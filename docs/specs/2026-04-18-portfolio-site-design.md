# Kaishi Portfolio Site — Design Spec

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** A single-page portfolio/landing site for Kaishi (懐紙), the Rust TUI for Hermes Agent. Deployed on Netlify via the existing GitHub repo (`eva-l3n4/kaishi`).

---

## Visual Identity

### Mood: Gentle Dusk

The page evokes watching cherry blossoms at twilight — purple-to-pink gradient sky, atmospheric and dreamy. The gradient flows through the entire page via full-bleed sections with alternating dark backgrounds.

### Color Palette

| Token | Value | Usage |
|---|---|---|
| `--dusk-deep` | `#2d1b33` | Darkest background, hero top |
| `--dusk-mid` | `#44274a` | Mid-gradient, section backgrounds |
| `--dusk-warm` | `#6b3a5e` | Warm transition zone |
| `--blossom` | `#d4889c` | Primary accent — links, highlights, borders |
| `--petal` | `#ffb7c5` | Bright accent — hover states, petal animation |
| `--cream` | `#fff0f3` | Primary text color |
| `--muted` | `#b8a0b0` | Secondary/body text |
| `--surface` | `#1e1228` | Card/section background (dark) |
| `--surface-alt` | `#241530` | Alternating section background |
| `--code-bg` | `#0d0a12` | Terminal/code block background |

### Typography

Three-font stack honoring the Japanese origin:

| Role | Font | Weight | Notes |
|---|---|---|---|
| Kanji (懐紙) | **Noto Serif JP** | 700 | Used only for the Japanese characters — gives them their own authentic voice |
| Headings & Body | **Inter** | 300, 400, 600 | Clean humanist sans-serif for all Latin text |
| Code / Terminal | **JetBrains Mono** | 400 | Monospace for install commands, terminal mock, inline code |

Google Fonts import:
```html
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;600&family=Noto+Serif+JP:wght@400;700&family=JetBrains+Mono:wght@400&display=swap" rel="stylesheet">
```

---

## Layout: Full-Bleed Sections

Each section stretches edge-to-edge with alternating dark backgrounds. The gradient hero flows into content sections. Content is max-width constrained (likely `800px`) and centered within each full-bleed section.

No navigation bar — the page is short enough to scroll naturally. A subtle "GitHub →" link in the hero or footer is sufficient.

---

## Sections (top to bottom)

### 1. Hero

- Full-viewport-height gradient background: `linear-gradient(180deg, #2d1b33 0%, #44274a 40%, #6b3a5e 70%, #d4889c 100%)`
- Centered content:
  - 懐紙 in Noto Serif JP, large (48–64px)
  - KAISHI in Inter 300, wide letter-spacing (4–6px)
  - Tagline: "A terminal for cherry blossom season" in Inter 400, `--blossom` color
- **Falling petal animation** (CSS-only, hero section only):
  - 8–12 small petal shapes (CSS pseudo-elements or small divs)
  - Gentle fall animation: top to bottom, slight horizontal drift, varying speeds (8–15s)
  - Opacity range 0.2–0.6, sizes 4–10px
  - `overflow: hidden` on the hero to contain petals
  - Petals are decorative — `pointer-events: none`, no accessibility concern

### 2. What is Kaishi?

- Background: `--surface` (`#1e1228`)
- Short paragraph (2–3 sentences max):
  - Kaishi is a terminal UI for conversing with Hermes Agent
  - Built in Rust with ratatui, speaks the ACP protocol
  - Syntax highlighting, session management, streaming responses
- Section heading in Inter 600

### 3. Features

- Background: `--surface-alt` (`#241530`)
- 3–4 feature cards in a horizontal row (flex, wrap on mobile to single column)
- Each card:
  - Subtle background: `rgba(255, 183, 197, 0.08)`
  - Border-radius: `8px`
  - Emoji or small icon (optional, keep minimal)
  - Feature name in Inter 600, `--blossom` color
  - One-line description in `--muted`
- Features to highlight:
  1. **Streaming** — Real-time token flow with thinking indicators
  2. **Syntax Highlighting** — Code blocks rendered with syntect
  3. **Session Management** — Pick up conversations where you left off
  4. **File Mentions** — @ to attach files with fuzzy autocomplete

### 4. Terminal Preview

- Background: `--surface` (`#1e1228`)
- A styled terminal mock showing a short Kaishi conversation:
  - Dark code background (`--code-bg`)
  - Rounded border with subtle `--blossom` border (`rgba(255,183,197,0.15)`)
  - Fake title bar with three dots (macOS-style window chrome)
  - Content must accurately reflect Kaishi's actual rendering:
    ```
    ┌──────────────────────────────────────────────────────┐
    │ claude-sonnet-4-6  │ [████░░░░░░] 42%               │  ← status bar (DarkGray bg)
    ├──────────────────────────────────────────────────────┤
    │  ❯ What's in this project?                          │  ← user (Cyan icon)
    │                                                      │
    │  ◆ This is a Rust TUI built with ratatui. It speaks │  ← assistant (Magenta icon)
    │    the ACP protocol to communicate with Hermes...   │
    │                                                      │
    │    ┌─ ✓ search_files ──────────────────────          │  ← tool call (Green ✓, box-drawn)
    │    │ Found 11 source files in src/                   │
    │    └─────────────────────────────────────────        │
    │                                                      │
    │  ──── 1.2k in · 247 out · 95% cached · 3s ────     │  ← turn summary (DarkGray)
    ├──────────────────────────────────────────────────────┤
    │ >                                                    │  ← input area
    └──────────────────────────────────────────────────────┘
    ```
  - Color coding matches Kaishi's actual palette:
    - `❯` user icon: Cyan
    - `◆` assistant icon: Magenta
    - `✓` tool success: Green (with box-drawing frame in DarkGray)
    - Turn summary divider: DarkGray
    - Status bar: DarkGray background, White text, Green/Yellow/Red context bar
    - Borders: DarkGray

### 5. Get Started

- Background: gradient mirror — `linear-gradient(180deg, #241530, #2d1b33)` (echoing the hero in reverse, creating a bookend)
- Centered content:
  - "Get Started" heading
  - Install command in a styled code block: `cargo install kaishi`
  - One-liner: "Then just run `kaishi` in your terminal."
  - GitHub link: styled as a subtle button/link with `→` arrow

### 6. Footer

- Same background as Get Started section (continuous)
- Minimal: "Built with 🌸 by Eva" or similar
- GitHub icon/link
- Very small, understated

---

## Technical Decisions

### Static HTML/CSS

- Single `index.html` file with embedded `<style>` block
- No build step, no framework, no JavaScript (except the optional petal animation if we decide to use JS — but CSS-only is preferred)
- Responsive: mobile-first, breakpoint at ~768px for feature cards

### File Structure

```
site/
  index.html       # The complete site
```

That's it. One file. Netlify serves `site/` as the publish directory.

### Netlify Deployment

- Connect the `eva-l3n4/kaishi` GitHub repo to Netlify
- Build settings:
  - Base directory: (empty)
  - Build command: (none)
  - Publish directory: `site`
- Auto-deploys on push to `main`
- Custom domain: optional/later (Netlify subdomain is fine for v1)

### .gitignore Update

Add `.superpowers/` to prevent brainstorm session files from being committed.

---

## What's Explicitly Out of Scope

- No dark/light mode toggle (it's always dark — the dusk theme IS the site)
- No JavaScript framework
- No blog, changelog, or multi-page navigation
- No analytics or tracking
- No custom domain setup (can add later)
- No screenshots of actual Kaishi (the terminal mock is hand-crafted HTML/CSS)
