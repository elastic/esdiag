#!/usr/bin/env bash

set -euo pipefail

# Fast recursive text replacement script
# Usage: ./replace_text.sh <directory>

if [ $# -eq 0 ]; then
    echo "Usage: $0 <directory>"
    echo "Replaces 'geocomply' with 'data' in all files recursively"
    exit 1
fi

TARGET_DIR="$1"

if [ ! -d "$TARGET_DIR" ]; then
    echo "Error: Directory '$TARGET_DIR' does not exist"
    exit 1
fi

echo "Replacing 'geocomply' with 'data' in all files under: $TARGET_DIR"

for file in $(find "$TARGET_DIR" -type f); do
    sed -i '' 's/geocomply/data/g' "$file"
done

echo "Replacement complete!"
