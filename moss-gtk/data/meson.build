# Linux-specific: desktop file, GSettings schema, and icons
if host_machine.system() == 'linux'
    subdir('icons')

    desktop_conf = configuration_data()
    desktop_conf.set('APP_ID', app_id)
    configure_file(
        input: 'app_entry.desktop.in',
        output: '@0@.desktop'.format(app_id),
        configuration: desktop_conf,
        install: true,
        install_dir: datadir / 'applications',
    )

    schemas_dir = datadir / 'glib-2.0' / 'schemas'
    install_emptydir(schemas_dir)
endif

subdir('resources')
