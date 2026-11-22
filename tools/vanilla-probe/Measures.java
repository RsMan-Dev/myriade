import java.util.*;

/** The behaviour measures of one run, defined as in `all_species_behaviour.rs::measures`, and their aggregation over the repetitions. */
final class Measures {
    // Row layout (same as the bench `Row`): position relative to the parcel, per-tick displacement, body yaw, health, alive, target, yaw, head yaw.
    static final int X = 0, Y = 1, Z = 2, VX = 3, VY = 4, VZ = 5, BODY = 6, HEALTH = 7, ALIVE = 8, TARGET = 9, YAW = 10, HEAD = 11;
    static final double MOVING = 0.001, PLAYER_X = 4.5, PLAYER_Z = 0.5;

    /** Metric name -> value; null value is a run without the event (`target_first`). Insertion order is the file order. */
    static LinkedHashMap<String, Double> of(List<double[]> rows, boolean withPlayer) {
        double[] first = rows.get(0), last = rows.get(rows.size() - 1);
        List<double[]> ticks = rows.subList(1, rows.size());
        double n = ticks.size();
        List<Double> speed = new ArrayList<>(), moving = new ArrayList<>();
        for (double[] r : ticks) {
            double s = Math.hypot(r[VX], r[VZ]);
            speed.add(s);
            if (s > MOVING) moving.add(s);
        }
        double meanMoving = moving.isEmpty() ? 0 : moving.stream().mapToDouble(d -> d).sum() / moving.size();
        double lo = Double.POSITIVE_INFINITY, hi = Double.NEGATIVE_INFINITY, path = 0, hops = 0;
        for (int i = 0; i < rows.size(); i++) {
            double[] r = rows.get(i);
            lo = Math.min(lo, r[Y]);
            hi = Math.max(hi, r[Y]);
            if (i > 0) {
                double[] p = rows.get(i - 1);
                path += Math.hypot(r[X] - p[X], r[Z] - p[Z]);
                if (p[VY] <= 0.0 && r[VY] > 0.1) hops++;
            }
        }
        int firstTarget = -1, targetTicks = 0;
        for (int i = 0; i < ticks.size(); i++) {
            if (ticks.get(i)[TARGET] == 1.0) {
                targetTicks++;
                if (firstTarget < 0) firstTarget = i + 1;
            }
        }
        LinkedHashMap<String, Double> m = new LinkedHashMap<>();
        m.put("path", path);
        m.put("net", Math.hypot(last[X] - first[X], last[Z] - first[Z]));
        m.put("moving", moving.size() / n);
        m.put("speed", meanMoving);
        m.put("top_speed", speed.stream().mapToDouble(d -> d).max().orElse(0));
        m.put("y_range", hi - lo);
        m.put("hops", hops);
        m.put("head_turn", turn(rows, HEAD));
        m.put("body_turn", turn(rows, BODY));
        m.put("target_ever", firstTarget >= 0 ? 1.0 : 0.0);
        m.put("target_ticks", targetTicks / n);
        m.put("target_first", firstTarget >= 0 ? (double) firstTarget : null);
        m.put("health_end", last[HEALTH]);
        m.put("alive_end", last[ALIVE]);
        if (withPlayer) {
            double min = Double.POSITIVE_INFINITY;
            for (double[] r : rows) min = Math.min(min, hdist(r));
            m.put("player_dist_end", hdist(last));
            m.put("player_dist_min", min);
        }
        return m;
    }

    static double hdist(double[] r) { return Math.hypot(r[X] - PLAYER_X, r[Z] - PLAYER_Z); }

    /** Shortest distance between two angles in degrees. */
    static double wrap(double d) {
        double r = ((d % 360.0) + 360.0) % 360.0;
        return Math.min(r, 360.0 - r);
    }

    static double turn(List<double[]> rows, int i) {
        double s = 0;
        for (int k = 1; k < rows.size(); k++) s += wrap(rows.get(k)[i] - rows.get(k - 1)[i]);
        return s;
    }

    /** One TSV line per metric: species scenario metric mean min max n, over the runs that have the value. */
    static void aggregate(String species, String scenario, List<LinkedHashMap<String, Double>> runs, StringBuilder out) {
        if (runs.isEmpty()) return;
        for (String name : runs.get(0).keySet()) {
            List<Double> v = new ArrayList<>();
            for (var r : runs) if (r.get(name) != null) v.add(r.get(name));
            out.append(species).append('\t').append(scenario).append('\t').append(name).append('\t');
            if (v.isEmpty()) {
                out.append("-\t-\t-\t0\n");
                continue;
            }
            double mean = v.stream().mapToDouble(d -> d).sum() / v.size();
            double mn = v.stream().mapToDouble(d -> d).min().getAsDouble(), mx = v.stream().mapToDouble(d -> d).max().getAsDouble();
            out.append(mean).append('\t').append(mn).append('\t').append(mx).append('\t').append(v.size()).append('\n');
        }
    }
}
