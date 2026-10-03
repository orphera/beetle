"""Bakes GPOS 'kern' pair adjustments for printable ASCII into a legacy
`kern` table (format 0), in place.

fontdue (our only rasterizer, AGENTS.md §3) reads kerning solely from the
legacy `kern` table, while Noto fonts only ship GPOS kerning — so without
this step Latin text renders with no kerning at all.

Usage: python scripts/add-latin-kern.py <font.ttf> [...]
Requires fontTools (developer machines only).
"""
import sys
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables._k_e_r_n import KernTable_format_0


def pair_value(subtable, left, right):
    cov = subtable.Coverage.glyphs
    if left not in cov:
        return None
    if subtable.Format == 1:
        pset = subtable.PairSet[cov.index(left)]
        for rec in pset.PairValueRecord:
            if rec.SecondGlyph == right:
                v = rec.Value1
                return getattr(v, "XAdvance", 0) if v else 0
        return None
    if subtable.Format == 2:
        c1 = subtable.ClassDef1.classDefs.get(left, 0)
        c2 = subtable.ClassDef2.classDefs.get(right, 0)
        v = subtable.Class1Record[c1].Class2Record[c2].Value1
        return getattr(v, "XAdvance", 0) if v else 0
    return None


def main(path):
    font = TTFont(path)
    gpos = font["GPOS"].table
    lookups = []
    for fr in gpos.FeatureList.FeatureRecord:
        if fr.FeatureTag == "kern":
            lookups += [gpos.LookupList.Lookup[i] for i in fr.Feature.LookupListIndex]
    subtables = []
    for lk in lookups:
        for st in lk.SubTable:
            if lk.LookupType == 9:  # extension wrapper
                st = st.ExtSubTable
            if getattr(st, "LookupType", 2) == 2 or lk.LookupType in (2, 9):
                subtables.append(st)

    cmap = font.getBestCmap()
    glyphs = sorted({cmap[c] for c in range(0x20, 0x7F) if c in cmap})
    pairs = {}
    for left in glyphs:
        for right in glyphs:
            for st in subtables:
                v = pair_value(st, left, right)
                if v is not None:  # first matching subtable wins, as in GPOS
                    if v:
                        pairs[(left, right)] = v
                    break

    kern = newTable("kern")
    kern.version = 0
    sub = KernTable_format_0()
    sub.version, sub.coverage, sub.format = 0, 1, 0
    sub.kernTable = pairs
    kern.kernTables = [sub]
    font["kern"] = kern
    font.save(path)
    print(f"{path}: {len(pairs)} Latin kerning pairs")


if __name__ == "__main__":
    for p in sys.argv[1:]:
        main(p)
