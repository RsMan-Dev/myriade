#!/usr/bin/env python3
"""Verdict of the bench rule between the vanilla reference TSV and a Myriade probe TSV (+ its .log for errors).
usage: compare.py VANILLA.tsv MYRIADE.tsv [OUT.tsv]"""
import sys, re, collections

def read(path):
    rows = {}
    for line in open(path).read().splitlines()[1:]:
        f = line.split("\t")
        if len(f) == 7 and f[3] != "-":
            rows[tuple(f[:3])] = tuple(float(x) for x in f[3:6])
    return rows

def differs(v, m):
    (vm, vlo, vhi), (mm, mlo, mhi) = v, m
    disjoint = mlo > vhi or mhi < vlo
    return disjoint and abs(vm - mm) > 0.25 * max(abs(vm), abs(mm)) and abs(vm - mm) > 0.01

vanilla, myriade = read(sys.argv[1]), read(sys.argv[2])
log = sys.argv[2].rsplit(".", 1)[0] + ".log"
errors = set()
try:
    errors = {m.group(1) for m in re.finditer(r"error (\S+) ", open(log).read())}
except OSError:
    pass
out, count = [], collections.defaultdict(lambda: collections.Counter())
for key in sorted(myriade):
    sp = key[0]
    if key not in vanilla:
        verdict, v = "no_reference", ("-",) * 3
    else:
        v = vanilla[key]
        verdict = "differs" if differs(v, myriade[key]) else "same"
    count[sp][verdict] += 1
    out.append("\t".join([*key, verdict, *map(str, v), *map(str, myriade[key])]))
for sp in sorted(errors):
    count[sp]["error"] += 1
    out.append(f"{sp}\t-\t-\terror\t-\t-\t-\t-\t-\t-")
lines = ["species\tscenario\tmetric\tverdict\tvanilla_mean\tvanilla_min\tvanilla_max\tmyriade_mean\tmyriade_min\tmyriade_max", *out]
tot = collections.Counter()
for sp in sorted(count):
    c = count[sp]; tot.update(c)
    lines.append(f"{sp} same={c['same']} differs={c['differs']} no_reference={c['no_reference']} error={c['error']}")
lines.append(f"TOTAL species={len(count)} same={tot['same']} differs={tot['differs']} no_reference={tot['no_reference']} error={tot['error']}")
text = "\n".join(lines) + "\n"
if len(sys.argv) > 3:
    open(sys.argv[3], "w").write(text)
print("\n".join(l for l in lines if " same=" in l or l.startswith("TOTAL")))
