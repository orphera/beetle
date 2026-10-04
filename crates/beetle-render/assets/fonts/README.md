# Embedded fonts

All fonts are SIL Open Font License 1.1 (see the `*-LICENSE.txt` files).

| File | Used by | Contents |
|---|---|---|
| `NotoSansKR-Common-Subset.ttf` | `src/text` (primary); `bpm-gui`'s bitmap font via `beetle_render::text::KR_BYTES` | Latin (ASCII, proportional, tabular digits) + 2,350 KS X 1001 Hangul + compatibility jamo |
| `NotoSansJP-Subset.ttf` | `src/text`; `bpm-gui` via `beetle_render::text::JP_BYTES` | Kana, ~2,137 joyo kanji, CJK punctuation, fullwidth forms |

## Post-processing

`fontdue` only reads kerning from the legacy `kern` table, while Noto fonts
ship GPOS kerning. After (re)creating `NotoSansKR-Common-Subset.ttf`, run:

```bash
python scripts/add-latin-kern.py crates/beetle-render/assets/fonts/NotoSansKR-Common-Subset.ttf
```

Characters missing from both subsets are rasterized on demand through
Windows GDI (`src/text/gdi.rs`), so subsets only need to cover common text.

`JetBrainsMono-*.ttf` moved to `crates/bpm-gui/assets/fonts` together with the
software bitmap font (only `bpm-gui` still renders on the CPU).
