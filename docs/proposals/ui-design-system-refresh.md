# Proposal: Commercial-Grade UI Design System (Option A — Extended Bitmap)

Status: APPROVED (user confirmed 2026-10-03)
Author: Hermes / beetle-dev

## Philosophy (updated from minimal to polished)
- Binary size stays minimal (<1MB per binary per AGENTS.md §1).
- External runtime font dependencies avoided; bitmap engine expanded.
- Cross-platform: GDI fallback for CJK on Windows; square-glyph fallback for CJK on Linux (no external font library dependency).
- Design tokens: consistent color palette (semantic roles), spacing scale, typography scale (Title/Value/Label/Hint), shadow/glow, border-radius rules.
- Component library: Panel, Card, Button, Badge, ProgressBar, Modal, HeaderBar.

## Font decision
- Option A selected: expanded bitmap (16x14 ASCII, 20x16 Hangul/Kana) + improved GDI CJK (18px, MS YaHei UI / 맑은 고딕).
- No new font-rasterizer crate (AGENTS.md §3 forbidden crates: fontdue, freetype, rusttype, cosmic-text). Cross-platform portability preserved.
- CJK readability improvement: GDI font size increased from -10px to -18px with CLEARTYPE_QUALITY; bitmap glyph resolution doubled for non-CJK text.

## Component & token plan
- Design tokens: base colors (surface/card/row/card-tinted/primary/accent/text-primary/text-secondary/text-tertiary/hint), spacing scale (s-based), typography (4 tiers), glow/shadow, border-radius, animation timing.
- Component library: standard Panel (bg contrast), Card (inner stat box), Button (functional border, selected state), Badge (label + value), ProgressBar (gauge-style), Modal (overlay + section header). All using existing tiny-skia + bitmap-font primitives (draw_rect, draw_text_with_shadow, draw_badge, blit_glyph_aa).

## Implementation order
1. docs/proposals/ui-design-system-refresh.md (this file)
2. docs/specs/ui_design_system.md — update with token definitions, component specs
3. Font engine: bitmap engine expansion (16x14 ASCII, bold variants) + GDI CJK polish
4. Design token module (ColorToken, SpacingScale, TypographyScale, Shadow)
5. Component library in beetle-render/src/components/
6. Screen-by-screen redesign (SongSelect → Result → KeyConfig → Gameplay HUD)
