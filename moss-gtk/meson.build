# Build configurations
gtk_exe_name = 'moss'
app_id = 'com.github.Moss'

# Cargo options
cargo = find_program('cargo')
cargo_options = ['--manifest-path', meson.current_source_dir() / 'Cargo.toml']
cargo_options += ['--target-dir', meson.project_build_root() / 'target']

if not is_devel
    cargo_options += ['--release']
    rust_target = 'release'
else
    rust_target = 'debug'
endif

custom_target(
    'build-moss-gtk',
    build_by_default: true,
    build_always_stale: true,
    output: gtk_exe_name,
    console: true,
    install: true,
    install_dir: bindir,
    env: {
        'APP_ID': app_id,
    },
    command: [
        cargo,
        'build',
        cargo_options,
        '&&',
        'cp',
        meson.project_build_root() / 'target' / rust_target / gtk_exe_name,
        '@OUTPUT@',
    ],
)
