# Build configurations
service_exe_name = 'mossd'
service_name = 'com.github.Mossd1'

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
    'build-mossd',
    build_by_default: true,
    build_always_stale: true,
    output: service_exe_name,
    console: true,
    install: true,
    install_dir: bindir,
    env: {
        'SERVICE_NAME': service_name,
    },
    command: [
        cargo,
        'build',
        cargo_options,
        '&&',
        'cp',
        meson.project_build_root() / 'target' / rust_target / service_exe_name,
        '@OUTPUT@',
    ],
)

# Generate and install the D-Bus service unit file
systemd = dependency('systemd', required: true)
systemd_system_unit_dir = systemd.get_variable(pkgconfig: 'systemdsystemunitdir')

configure_file(
    input: 'resources/systemd_service/systemd.service.in',
    output: service_exe_name + '.service',
    configuration: {
        'BINDIR': bindir,
        'SERVICE_EXE_NAME': service_exe_name,
        'SERVICE_NAME': service_name,
    },
    install: true,
    install_dir: systemd_system_unit_dir
)

# D-Bus policy file
dbus_policy_path = datadir / 'dbus-1' / 'system.d'

configure_file(
    input: 'resources/dbus_policy/policy.conf.in',
    output: service_name + '.conf',
    configuration: {
        'SERVICE_NAME': service_name,
    },
    install: true,
    install_dir: dbus_policy_path
)

# D-Bus activation file
dbus = dependency('dbus-1', required: true)
dbus_system_services_dir = dbus.get_variable(pkgconfig: 'system_bus_services_dir')

configure_file(
    input: 'resources/dbus_service/dbus.service.in',
    output: service_name + '.service',
    configuration: {
        'BINDIR': bindir,
        'SERVICE_EXE_NAME': service_exe_name,
        'SERVICE_NAME': service_name,
    },
    install: true,
    install_dir: dbus_system_services_dir
)
