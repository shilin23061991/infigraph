#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUT_DIR="$SCRIPT_DIR/target"
LIB_DIR="$SCRIPT_DIR/lib"
ANTLR_JAR="$LIB_DIR/antlr4-complete.jar"

mkdir -p "$LIB_DIR"

# Download dependencies if not present
if [ ! -f "$ANTLR_JAR" ]; then
    echo "Downloading ANTLR4 runtime..."
    curl -sL -o "$ANTLR_JAR" "https://www.antlr.org/download/antlr-4.13.2-complete.jar"
fi

# Check mcpp is available (MSVC-compatible preprocessor, cross-platform)
if ! command -v mcpp &> /dev/null; then
    echo "WARNING: 'mcpp' (MSVC-compatible C preprocessor) not found in PATH."
    echo "  macOS:    brew install mcpp"
    echo "  Linux:    apt install mcpp"
    echo "  Windows:  pacman -S mcpp (MSYS2)"
    echo "  Grammars with preprocessor=c will fail at runtime without mcpp."
fi

CP="$ANTLR_JAR"

# Compile
mkdir -p "$OUT_DIR"
echo "Compiling GrammarDriver..."
EXTRACTOR_DIR="$SCRIPT_DIR/src/main/java/com/infigraph/driver/extractors"
EXTRACTOR_FILES=("$EXTRACTOR_DIR/BaseExtractor.java")
for f in "$EXTRACTOR_DIR"/*Extractor.java; do
    [[ "$f" == *BaseExtractor.java ]] && continue
    EXTRACTOR_FILES+=("$f")
done

javac -cp "$CP" \
    -d "$OUT_DIR" \
    "${EXTRACTOR_FILES[@]}" \
    "$SCRIPT_DIR/src/main/java/com/infigraph/driver/GrammarDriver.java"

# Create fat jar with all dependencies bundled
echo "Building infigraph-driver.jar..."
cd "$OUT_DIR"

# Extract dependency classes
jar xf "$ANTLR_JAR"

# Create manifest
mkdir -p META-INF
echo "Main-Class: com.infigraph.driver.GrammarDriver" > META-INF/MANIFEST.MF

# Build jar
jar cfm "$SCRIPT_DIR/infigraph-driver.jar" META-INF/MANIFEST.MF \
    com/ org/ META-INF/

echo "Built: $SCRIPT_DIR/infigraph-driver.jar"
echo "Run:   java -jar $SCRIPT_DIR/infigraph-driver.jar"
