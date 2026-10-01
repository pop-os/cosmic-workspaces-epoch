name := 'cosmic-workspaces'
APPID := 'com.system76.CosmicWorkspaces'
rootdir := ''
prefix := '/usr'
profile := 'release'
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')

mod cargo 'cargo.just'

base-dir := absolute_path(clean(rootdir / prefix))
bin-dst := base-dir / 'bin' / name
desktop-dst := base-dir / 'share/applications' / (APPID + '.desktop')
icon-dst := base-dir / 'share/icons/hicolor/scalable/apps' / (APPID + '.svg')

# Compile with release profile by default
default: build-release

# Compile with debug profile
build-debug *args: (cargo::build-debug args)

# Compile with release profile
build-release *args: (cargo::build-release args)

# Compile with a vendored tarball
build-vendored *args: (cargo::build-vendored args)

# Remove Cargo build artifacts
clean: cargo::clean

# Also remove .cargo and vendored dependencies
clean-dist: cargo::clean-dist

# Install the binary, desktop entry, and icon (use profile=debug for debug builds)
install:
    install -Dm0755 '{{ cargo-target-dir / profile / name }}' '{{ bin-dst }}'
    install -Dm0644 'data/{{ APPID }}.desktop' '{{ desktop-dst }}'
    install -Dm0644 'data/{{ APPID }}.svg' '{{ icon-dst }}'

# Vendor Cargo dependencies locally
vendor: cargo::vendor

# Extract vendored dependencies
vendor-extract: cargo::vendor-extract
