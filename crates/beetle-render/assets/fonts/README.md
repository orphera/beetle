# Embedded fonts

All fonts are SIL Open Font License 1.1 (see the `*-LICENSE.txt` files).

| File | Used by | Contents |
|---|---|---|
| `NotoSansKR-Common-Subset.ttf` | `src/text` (primary), legacy `bitmap_font` | Latin (ASCII, proportional, tabular digits) + 2,350 KS X 1001 Hangul + compatibility jamo |
| `NotoSansJP-Subset.ttf` | `src/text`, legacy `bitmap_font` | Kana, ~2,137 joyo kanji, CJK punctuation, fullwidth forms |
| `JetBrainsMono-*.ttf` | legacy `bitmap_font` only | Monospace Latin; removed together with the software renderer (P4 of docs/plans/2026-10-04-d3d11-ui-rebuild.md) |

## Post-processing

`fontdue` only reads kerning from the legacy `kern` table, while Noto fonts
ship GPOS kerning. After (re)creating `NotoSansKR-Common-Subset.ttf`, run:

```bash
python scripts/add-latin-kern.py crates/beetle-render/assets/fonts/NotoSansKR-Common-Subset.ttf
```

Characters missing from both subsets are rasterized on demand through
Windows GDI (`src/text/gdi.rs`), so subsets only need to cover common text.
