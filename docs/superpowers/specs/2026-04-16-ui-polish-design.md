# UI Polish — Full CSS Refresh

**Date:** 2026-04-16
**File:** `crates/claude-code-transcripts-ingest/web/index.html`
**Scope:** CSS-only changes (the `<style>` block and one new `<link>` tag). No React component restructuring.

## Goal

The UI looks functional but unpolished. Issues: system fonts read as placeholder, elements are too small, flat cards lack depth, header controls feel like raw form elements. The dark GitHub aesthetic stays — this is a refinement, not a redesign.

---

## 1. Typography

**Add to `<head>`:**
```html
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;600&display=swap" rel="stylesheet">
```

**Font assignments:**
- `body`: `'Inter', -apple-system, BlinkMacSystemFont, sans-serif`
- Numeric/code data (token counts, costs, API numbers, bar values): `'JetBrains Mono', monospace`
  - Applied to: `.api-cost`, `.bar-value`, `.tok-stats`, `.stat-value`, `.metric-value`, `.data-table td.num`, `.data-table td.cost`, `.totals`

**Size changes** (scale all descendants proportionally):
- `body`: `13px → 14px`
- `.panel-title`: `12px → 13px`
- `.stat-value`: `20px → 24px`
- `.metric-value`: `18px → 22px`
- `.pill`: `11px → 12px`
- `.api-num`: `11px → 12px`
- `.api-model`: `10px → 11px`
- `.api-cost`: `11px → 12px`
- `.bar-label`: `9px → 10px`
- `.tok-stats`: `10px → 11px`
- `.stat-label`: `10px → 11px`
- `.metric-label`: `10px → 11px`

---

## 2. Sizing & Spacing

**Global radius:** `--radius: 6px → 8px`

**Token bars:**
- `.bar-track`: height `9px → 14px`
- `.bar-track`: `border-radius: 2px → 3px`

**Pills:**
- `.pill`: padding `2px 7px → 4px 10px`
- `.pill`: `border-radius: 10px → 12px`

**Cards:**
- `.card`: padding `10px 12px → 14px 16px`

**Timeline:**
- `.timeline`: gap `6px → 10px`

**Stat cards:**
- `.stat-card`: padding `12px 14px → 16px 18px`

**Header controls (buttons, select):**
- `.sort-btn`: padding `4px 9px → 5px 11px`
- `.tab-btn`: padding `5px 14px → 6px 16px`
- `.btn`: padding `5px 14px → 6px 16px`
- `.header select`: padding `5px 8px → 6px 10px`

---

## 3. Depth & Color

**Card/panel shadows:**
```css
.card, .panel, .stat-card {
  box-shadow: 0 1px 3px rgba(0,0,0,0.4), 0 2px 8px rgba(0,0,0,0.25);
}
```

**Panel gradient sheen:**
```css
.panel {
  background: linear-gradient(180deg, rgba(255,255,255,0.025) 0%, transparent 60%),
              var(--surface);
}
```

**Stat card accent line:**
```css
.stat-card {
  border-top: 2px solid rgba(88,166,255,0.2);
}
```

**Active state glow ring:**
```css
.sort-btn.active, .tab-btn.active {
  box-shadow: 0 0 0 1px rgba(88,166,255,0.4);
}
```

**Subagent card:**
```css
.card-subagent {
  background: #0d1117;  /* was #111820, now matches --bg for clear distinction */
}
```

---

## 4. Header Controls — Grouped Segmented Controls

**Sort group** — remove gap between buttons, shared border treatment:
```css
.sort-group {
  gap: 0;  /* was 2px */
}
.sort-btn {
  border-radius: 0;
  border-right-width: 0;
}
.sort-btn:first-child {
  border-radius: var(--radius) 0 0 var(--radius);
}
.sort-btn:last-child {
  border-radius: 0 var(--radius) var(--radius) 0;
  border-right-width: 1px;
}
```

**Tab nav** — same grouped treatment:
```css
.tab-nav {
  gap: 0;  /* was 2px */
}
.tab-btn {
  border-radius: 0;
  border-right-width: 0;
}
.tab-btn:first-child {
  border-radius: var(--radius) 0 0 var(--radius);
}
.tab-btn:last-child {
  border-radius: 0 var(--radius) var(--radius) 0;
  border-right-width: 1px;
}
```

---

## Out of Scope

- No React component changes
- No color palette changes (all `--tok-*` and semantic colors stay)
- No layout restructuring
- No light mode
- No animation changes
