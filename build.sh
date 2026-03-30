#!/bin/bash
set -e

rm -rf archive.bin ./public/

echo "🦀 Building WASM Engine..."
cargo build --release --lib --target wasm32-unknown-unknown

echo "📦 Generating JS Bindings..."
wasm-bindgen target/wasm32-unknown-unknown/release/discord_wasm_viewer.wasm --out-dir pkg --target no-modules

echo "🚀 Running Generator..."
cargo run --release --bin generator -- ".."

echo "📄 Assembling viewer.html..."
python3 -c "
import base64

with open('pkg/discord_wasm_viewer.js', 'r') as f:
    js_glue = f.read()

with open('pkg/discord_wasm_viewer_bg.wasm', 'rb') as f:
    wasm_b64 = base64.b64encode(f.read()).decode('utf-8')

with open('template.html', 'r') as f:
    html = f.read()

svg = '<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><circle cx=\"12\" cy=\"12\" r=\"12\" fill=\"#5865F2\"/><path fill=\"#fff\" d=\"M12 14c-4 0-7 2-7 5v1h14v-1c0-3-3-5-7-5zm0-9c-2.2 0-4 1.8-4 4s1.8 4 4 4 4-1.8 4-4-1.8-4-4-4z\"/></svg>'
svg_b64 = base64.b64encode(svg.encode('utf-8')).decode('utf-8')
default_pfp = f'data:image/svg+xml;base64,{svg_b64}'

html = html.replace('__WASM_GLUE__', js_glue)
html = html.replace('__WASM_B64__', wasm_b64)
html = html.replace('__DEFAULT_PFP__', default_pfp)

with open('viewer.html', 'w') as f:
    f.write(html)
"

echo "📁 Packaging output..."
mkdir -p public
cp viewer.html public/index.html

echo "✅ Done! Final output is in the ./public/ directory."
