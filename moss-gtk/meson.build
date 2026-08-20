# Build configurations
gtk_exe_name = 'moss'

base_id = 'com.github.Moss'
if is_devel
    app_id = '@0@Dev'.format(base_id)
else
    app_id = base_id 
endif

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

# Import GNOME module
gnome = import('gnome')

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

subdir('data')

# Execute these tasks after installing
gnome.post_install(
    glib_compile_schemas: true,
    update_desktop_database: true,
)
