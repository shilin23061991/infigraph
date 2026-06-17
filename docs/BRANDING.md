---
layout: default
title: Branding Setup
parent: Contributing
nav_order: 99
---

# Branding Setup for Infigraph Docs

This document describes how the Infigraph branding system is integrated with the Jekyll documentation site.

## Asset Organization

Branding assets are organized in `/branding-system/` with the following structure:

```
branding-system/
├── logos/                          # Primary logos (horizontal)
│   ├── infigraph-logo.svg         # Main logo (SVG - preferred)
│   ├── infigraph-logo.png         # Main logo (PNG fallback)
│   ├── infigraph-light.png        # Horizontal light variant
│   └── infigraph-dark.png         # Horizontal dark variant
├── vertical/                       # Vertical variants (for social media)
│   ├── infigraph-vertical-light.png
│   └── infigraph-vertical-dark.png
└── banners/                        # Footer/bottom banners
    ├── bottom-banner1-light.png
    ├── bottom-banner1-dark.png
    ├── bottom-banner2-light.png
    ├── bottom-banner2-dark.png
    ├── bottom-banner3-light.png
    └── bottom-banner3-dark.png
```

## Jekyll Integration

### Logo in Navbar

The logo is configured in `docs/_config.yml`:

```yaml
logo: /infigraph/assets/branding/logo.svg
```

The SVG is copied to `docs/assets/branding/logo.svg` during documentation build and displayed in the Just the Docs navbar header.

### Custom Styling

Custom CSS for branding elements is defined in:
- `docs/assets/css/style.scss` — Main stylesheet (imports Just the Docs theme + custom styles)
- `docs/_sass/custom/custom.scss` — Branding-specific styles

The custom styles ensure:
- Proper hero banner sizing and spacing
- Responsive images on mobile devices
- Footer banner styling
- Logo sizing in navbar
- Dark mode compatibility

### Banner Placement

- **Hero banner** (`hero-banner.png`) — Top of landing page (`docs/index.md`)
- **Footer banner** (`footer-banner.png`) — Bottom of landing page before closing

These use markdown image syntax and are styled via `_sass/custom/custom.scss`.

## Updating Branding

### To change the logo in navbar:

1. Replace or update file in `branding-system/logos/`
2. Copy to `docs/assets/branding/logo.svg`
3. No config changes needed (path stays the same)

### To change hero/footer banners:

1. Update files in `branding-system/banners/`
2. Copy new files to `docs/assets/branding/`
3. Update paths in `docs/index.md` if file names change

### To adjust styling:

Edit `docs/_sass/custom/custom.scss` for:
- Banner sizing
- Spacing and margins
- Responsive breakpoints
- Dark mode appearance

## Build & Deploy

The Jekyll site builds automatically:

1. Assets in `docs/assets/branding/` are included in the build
2. Custom CSS is compiled from `docs/_sass/custom/custom.scss`
3. Images display with proper responsive sizing

To build locally:

```bash
cd docs
bundle install
bundle exec jekyll serve
```

Visit `http://localhost:4000/infigraph/` to see branding in action.

## Theme

The docs site uses **Just the Docs** theme with custom branding overlays. Theme configuration in `docs/_config.yml`:

- `color_scheme: light` — Light theme by default
- `logo` — Logo in navbar
- `back_to_top: true` — Back-to-top button on long pages
- Custom CSS via `style.scss`

For more details, see [Just the Docs Documentation](https://just-the-docs.github.io/).
