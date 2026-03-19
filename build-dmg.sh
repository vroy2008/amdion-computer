#!/bin/bash

# Navigate to the electron-app directory
cd "$(dirname "$0")"

echo "📦 Packaging AMDION Desktop App for macOS..."

# Ensure dependencies are installed
echo "Installing dependencies..."
npm install

# Run electron-builder for mac
echo "Building the .dmg installer..."
npx electron-builder --mac --config.mac.target=dmg

echo "✅ Build complete! You can find your .dmg installer in the 'dist' folder."
