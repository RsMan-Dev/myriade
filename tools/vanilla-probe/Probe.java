import com.mojang.authlib.GameProfile;
import io.netty.channel.*;
import io.netty.channel.embedded.EmbeddedChannel;
import io.netty.util.ReferenceCountUtil;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BooleanSupplier;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.Connection;
import net.minecraft.network.protocol.PacketFlow;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.*;
import net.minecraft.server.network.CommonListenerCookie;
import net.minecraft.world.Difficulty;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.memory.MemoryModuleType;
import net.minecraft.world.entity.ai.navigation.WaterBoundPathNavigation;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;

/** Main class in front of the vanilla jar: starts the dedicated server, then plays the behaviour runs on the server thread. */
public class Probe {
    static final int TICKS = Integer.getInteger("probe.ticks", 400), REPS = Integer.getInteger("probe.reps", 8);
    static final Path OUT = Path.of(System.getProperty("probe.out", "vanilla-behaviour.tsv"));
    static final Set<String> ONLY = new HashSet<>(Arrays.asList(System.getProperty("probe.species", "").split(",")));
    static final Long SEED = System.getenv("PROBE_SEED") == null ? null : Long.parseLong(System.getenv("PROBE_SEED"));
    static MinecraftServer server;
    static ServerLevel lv;
    static Method tickServer;
    static int[] ox = new int[REPS], oz = new int[REPS], surface = new int[REPS];
    static ServerPlayer[] players = new ServerPlayer[REPS];

    /** A player that cannot be hurt (health left intact, still targetable: not flagged invulnerable). */
    static final class Dummy extends ServerPlayer {
        Dummy(MinecraftServer s, ServerLevel l, GameProfile p) { super(s, l, p, ClientInformation.createDefault()); }
        @Override public boolean hurtServer(ServerLevel l, DamageSource d, float a) { return false; }
    }

    public static void main(String[] args) throws Exception {
        Thread w = new Thread(Probe::watch, "probe-watch");
        w.start();
        net.minecraft.server.Main.main(args);
    }

    static void watch() {
        try {
            long end = System.currentTimeMillis() + 300_000;
            while (server == null && System.currentTimeMillis() < end) {
                Thread.sleep(500);
                server = find();
            }
            while (server != null && server.getTickCount() < 10 && System.currentTimeMillis() < end) Thread.sleep(500);
            if (server == null) throw new IllegalStateException("server not found");
            MinecraftServer s = server;
            s.execute(() -> {
                try { play(); } catch (Throwable t) { log("fatal " + t); t.printStackTrace(); } finally { s.halt(false); }
            });
            Thread.sleep(2000);
            Thread st = null;
            for (Thread t : Thread.getAllStackTraces().keySet()) if (t.getName().equals("Server thread")) st = t;
            if (st != null) st.join(900_000);
        } catch (Throwable t) { t.printStackTrace(); }
        System.exit(0);
    }

    static MinecraftServer find() throws Exception {
        for (Thread th : Thread.getAllStackTraces().keySet()) {
            if (!th.getName().equals("Server thread")) continue;
            Field h = Thread.class.getDeclaredField("holder");
            h.setAccessible(true);
            Object holder = h.get(th);
            Field tf = holder.getClass().getDeclaredField("task");
            tf.setAccessible(true);
            Object task = tf.get(holder);
            for (Field f : task.getClass().getDeclaredFields()) {
                f.setAccessible(true);
                if (f.get(task) instanceof AtomicReference<?> r && r.get() instanceof MinecraftServer m) return m;
            }
        }
        return null;
    }

    static void log(String s) { System.out.println("[probe] " + s); }

    static void tick() throws Exception { tickServer.invoke(server, (BooleanSupplier) () -> true); }

    static void cmd(String c) { server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), c); }

    static void play() throws Throwable {
        long t0 = System.currentTimeMillis();
        lv = server.overworld();
        tickServer = MinecraftServer.class.getDeclaredMethod("tickServer", BooleanSupplier.class);
        tickServer.setAccessible(true);
        server.setDifficulty(Difficulty.NORMAL, true);
        for (String c : new String[] {"gamerule spawn_mobs false", "gamerule spawn_monsters false", "gamerule spawn_patrols false", "gamerule spawn_phantoms false",
                "gamerule spawn_wandering_traders false", "gamerule spawn_wardens false", "gamerule advance_time false", "gamerule advance_weather false",
                "gamerule mob_griefing false", "time set " + System.getenv().getOrDefault("PROBE_TIME", "18000"), "weather clear", "save-off"}) cmd(c);
        for (int i = 0; i < REPS; i++) {
            ox[i] = 1024 + (i % 4) * 160;
            oz[i] = 1024 + (i / 4) * 160;
            for (int dx = -4; dx <= 4; dx++) for (int dz = -4; dz <= 4; dz++) {
                lv.setChunkForced((ox[i] >> 4) + dx, (oz[i] >> 4) + dz, true);
                lv.getChunk((ox[i] >> 4) + dx, (oz[i] >> 4) + dz);
            }
            surface[i] = lv.getHeight(Heightmap.Types.MOTION_BLOCKING, ox[i], oz[i]);
        }
        for (int i = 0; i < 40; i++) tick();
        log("parcel 0 surface y=" + surface[0] + " entityTicking=" + lv.isPositionEntityTicking(new BlockPos(ox[0], surface[0], oz[0])));
        List<EntityType<?>> types = new ArrayList<>();
        for (EntityType<?> t : BuiltInRegistries.ENTITY_TYPE) {
            String id = BuiltInRegistries.ENTITY_TYPE.getKey(t).toString();
            if (t != EntityTypes.PLAYER && (ONLY.contains("") || ONLY.contains(id))) types.add(t);
        }
        Map<String, Boolean> pool = new HashMap<>();
        Map<String, String> errors = new LinkedHashMap<>();
        List<String> skipped = new ArrayList<>();
        StringBuilder out = new StringBuilder("species\tscenario\tmetric\tmean\tmin\tmax\tn\n");
        Map<String, StringBuilder> alone = new LinkedHashMap<>(), withPlayer = new LinkedHashMap<>();
        for (boolean player : new boolean[] {false, true}) {
            if (player) addPlayers();
            for (EntityType<?> t : types) {
                String id = BuiltInRegistries.ENTITY_TYPE.getKey(t).toString();
                if (errors.containsKey(id)) continue;
                try {
                    if (!player) {
                        Entity probe = t.create(lv, EntitySpawnReason.COMMAND);
                        boolean living = probe instanceof LivingEntity && (t.getCategory() != MobCategory.MISC || probe instanceof Mob);
                        if (!living) { skipped.add(id + (probe == null ? "(null)" : "")); errors.put(id, null); continue; }
                        MobCategory c = t.getCategory();
                        pool.put(id, probe instanceof Mob m && (c == MobCategory.WATER_CREATURE || c == MobCategory.WATER_AMBIENT
                                || c == MobCategory.UNDERGROUND_WATER_CREATURE || m.getNavigation() instanceof WaterBoundPathNavigation));
                    }
                    var runs = batch(t, player, pool.get(id));
                    StringBuilder sb = new StringBuilder();
                    Measures.aggregate(id, player ? "player" : "alone", runs, sb);
                    (player ? withPlayer : alone).put(id, sb);
                } catch (Throwable e) {
                    Throwable r = e instanceof InvocationTargetException ? e.getCause() : e;
                    errors.put(id, (player ? "player: " : "alone: ") + r.toString().replace('\t', ' ').replace('\n', ' '));
                    log("error " + id + " " + r);
                    try { clear(); tick(); } catch (Throwable ignored) { }
                }
            }
            log("phase " + (player ? "player" : "alone") + " done after " + (System.currentTimeMillis() - t0) / 1000 + " s");
        }
        for (String id : alone.keySet()) out.append(alone.get(id)).append(withPlayer.getOrDefault(id, new StringBuilder()));
        for (var e : errors.entrySet()) if (e.getValue() != null) out.append(e.getKey()).append("\t-\terror\t").append(e.getValue()).append('\n');
        Files.writeString(OUT, out.toString());
        log("measured=" + alone.size() + " errors=" + errors.values().stream().filter(Objects::nonNull).count() + " skipped_non_living=" + skipped.size() + " " + skipped);
        log("pool species: " + pool.entrySet().stream().filter(Map.Entry::getValue).map(Map.Entry::getKey).sorted().toList());
        log("elapsed " + (System.currentTimeMillis() - t0) / 1000 + " s");
    }

    /** One immobile player per parcel, with a connection that drops everything it is sent. */
    static void addPlayers() throws Exception {
        for (int i = 0; i < REPS; i++) {
            GameProfile gp = new GameProfile(UUID.nameUUIDFromBytes(("probe" + i).getBytes()), "Probe" + i);
            Dummy p = new Dummy(server, lv, gp);
            Connection c = new Connection(PacketFlow.SERVERBOUND);
            ChannelOutboundHandlerAdapter drop = new ChannelOutboundHandlerAdapter() {
                @Override public void write(ChannelHandlerContext x, Object m, ChannelPromise pr) { ReferenceCountUtil.release(m); pr.setSuccess(); }
            };
            new EmbeddedChannel(new ChannelInitializer<Channel>() {
                @Override protected void initChannel(Channel ch) {
                    ch.pipeline().addLast("drop", drop);
                    for (String n : new String[] {"encoder", "outbound_config", "decoder", "inbound_config"}) ch.pipeline().addLast(n, new ChannelDuplexHandler());
                    ch.pipeline().addLast("packet_handler", c);
                }
            });
            server.getPlayerList().placeNewPlayer(c, p, CommonListenerCookie.createInitial(gp, false));
            players[i] = p;
        }
        for (int i = 0; i < 5; i++) tick();
        log("players added: " + lv.players().size());
    }

    static void clear() {
        for (Entity e : new ArrayList<>(iter(lv.getAllEntities()))) if (!(e instanceof ServerPlayer)) e.discard();
        for (int i = 0; i < REPS; i++) for (int dx = -2; dx <= 2; dx++) for (int dz = -3; dz <= 3; dz++) for (int y = 0; y <= 1; y++)
            lv.setBlock(new BlockPos(ox[i] + dx, surface[i] + y, oz[i] + dz), Blocks.AIR.defaultBlockState(), Block.UPDATE_CLIENTS | Block.UPDATE_SKIP_ON_PLACE);
    }

    static <T> List<T> iter(Iterable<T> it) { List<T> l = new ArrayList<>(); it.forEach(l::add); return l; }

    static boolean target(LivingEntity e) {
        if (!(e instanceof Mob m)) return false;
        return m.getTarget() != null || m.getBrain().hasMemoryValue(MemoryModuleType.ATTACK_TARGET);
    }

    static double[] row(LivingEntity e, int i, double[] prev) {
        double x = e.getX() - ox[i], y = e.getY() - surface[i] + 1, z = e.getZ() - oz[i];
        double[] r = new double[12];
        r[Measures.X] = x; r[Measures.Y] = y; r[Measures.Z] = z;
        if (prev != null) { r[Measures.VX] = x - prev[0]; r[Measures.VY] = y - prev[1]; r[Measures.VZ] = z - prev[2]; }
        r[Measures.BODY] = e.yBodyRot;
        r[Measures.HEALTH] = e.getHealth();
        r[Measures.ALIVE] = e.isAlive() ? 1 : 0;
        r[Measures.TARGET] = target(e) ? 1 : 0;
        r[Measures.YAW] = e.getYRot();
        r[Measures.HEAD] = e.yHeadRot;
        return r;
    }

    /** REPS parcels of one species at once: spawn, TICKS ticks (or until each is dead), one row list per repetition. */
    static List<LinkedHashMap<String, Double>> batch(EntityType<?> t, boolean withPlayer, boolean pool) throws Exception {
        clear();
        tick();
        LivingEntity[] es = new LivingEntity[REPS];
        List<List<double[]>> rows = new ArrayList<>();
        for (int i = 0; i < REPS; i++) {
            if (pool) for (int dx = -2; dx <= 2; dx++) for (int dz = -3; dz <= 3; dz++) for (int y = 0; y <= 1; y++)
                lv.setBlock(new BlockPos(ox[i] + dx, surface[i] + y, oz[i] + dz), Blocks.WATER.defaultBlockState(), Block.UPDATE_CLIENTS | Block.UPDATE_SKIP_ON_PLACE);
            Entity e = t.create(lv, EntitySpawnReason.COMMAND);
            if (e == null) throw new IllegalStateException("EntityType.create returned null");
            e.snapTo(ox[i] + 0.5, surface[i], oz[i] + 0.5, 0f, 0f);
            if (SEED != null) {
                Field rf = Entity.class.getDeclaredField("random");
                rf.setAccessible(true);
                rf.set(e, net.minecraft.util.RandomSource.create(SEED + 1000L * i + t.hashCode()));
            }
            if (e instanceof Mob m) {
                m.finalizeSpawn(lv, lv.getCurrentDifficultyAt(new BlockPos(ox[i], surface[i], oz[i])), EntitySpawnReason.COMMAND, null);
                if (m.isBaby()) m.setBaby(false);
            }
            Entity v = e.getVehicle();
            e.stopRiding();
            if (v != null) v.discard();
            for (Entity p : new ArrayList<>(e.getPassengers())) { p.stopRiding(); p.discard(); }
            e.setYRot(0f);
            if (e instanceof LivingEntity le) { le.setYBodyRot(0f); le.setYHeadRot(0f); }
            lv.addFreshEntity(e);
            es[i] = (LivingEntity) e;
            rows.add(new ArrayList<>(List.of(row(es[i], i, null))));
        }
        boolean[] on = new boolean[REPS];
        Arrays.fill(on, true);
        for (int k = 0; k < TICKS && on.length > 0; k++) {
            if (withPlayer) for (int i = 0; i < REPS; i++) {
                ServerPlayer p = players[i];
                p.snapTo(ox[i] + 4.5, surface[i], oz[i] + 0.5, 0f, 0f);
                p.setDeltaMovement(Vec3.ZERO);
                p.setHealth(p.getMaxHealth());
            }
            tick();
            boolean any = false;
            for (int i = 0; i < REPS; i++) {
                if (!on[i]) continue;
                List<double[]> r = rows.get(i);
                r.add(row(es[i], i, r.get(r.size() - 1)));
                on[i] = r.get(r.size() - 1)[Measures.ALIVE] == 1.0;
                any |= on[i];
            }
            if (!any) break;
        }
        List<LinkedHashMap<String, Double>> out = new ArrayList<>();
        for (int i = 0; i < REPS; i++) if (rows.get(i).size() > 1) out.add(Measures.of(rows.get(i), withPlayer));
        return out;
    }
}
