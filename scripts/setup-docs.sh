#!/bin/bash
# Setup script for Jekyll documentation site
# Copies branding assets from branding-system/ to docs/assets/branding/
# Run this once after cloning: ./scripts/setup-docs.sh

set -e  # Exit on any error

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

BRANDING_SOURCE="$PROJECT_ROOT/branding-system"
DOCS_ASSETS="$PROJECT_ROOT/docs/assets/branding"

echo "=========================================="
echo "Setting up Jekyll documentation site"
echo "=========================================="
echo ""

# Check if branding-system exists
if [ ! -d "$BRANDING_SOURCE" ]; then
  echo "❌ Error: branding-system directory not found at $BRANDING_SOURCE"
  exit 1
fi

# Create destination directory
mkdir -p "$DOCS_ASSETS"
echo "✓ Created $DOCS_ASSETS/"

# Copy logo (SVG - preferred for navbar)
if [ -f "$BRANDING_SOURCE/logos/infigraph-logo.svg" ]; then
  cp "$BRANDING_SOURCE/logos/infigraph-logo.svg" "$DOCS_ASSETS/logo.svg"
  echo "✓ Copied logo.svg"
else
  echo "❌ Error: logo.svg not found"
  exit 1
fi

# Copy hero banner (light variant)
if [ -f "$BRANDING_SOURCE/logos/infigraph-light.png" ]; then
  cp "$BRANDING_SOURCE/logos/infigraph-light.png" "$DOCS_ASSETS/hero-banner.png"
  echo "✓ Copied hero-banner.png"
else
  echo "❌ Error: infigraph-light.png not found"
  exit 1
fi

# Copy footer banner (light variant)
if [ -f "$BRANDING_SOURCE/banners/bottom-banner1-light.png" ]; then
  cp "$BRANDING_SOURCE/banners/bottom-banner1-light.png" "$DOCS_ASSETS/footer-banner.png"
  echo "✓ Copied footer-banner.png"
else
  echo "❌ Error: bottom-banner1-light.png not found"
  exit 1
fi

echo ""
echo "=========================================="
echo "✅ Setup complete!"
echo "=========================================="
echo ""
echo "Next steps:"
echo "  1. cd docs"
echo "  2. bundle install"
echo "  3. bundle exec jekyll serve"
echo ""
echo "Site will be available at: http://localhost:4000/infigraph/"
