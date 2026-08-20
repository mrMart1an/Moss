project('moss', 'rust', version: '0.1.0', meson_version: '>= 1.4')

# Check if the build profile is development
is_devel = get_option('profile') == 'development'

# Installation paths
bindir = get_option('prefix') / get_option('bindir')
datadir = get_option('prefix') / get_option('datadir')
pkgdatadir = datadir / meson.project_name()

if get_option('mossd')
    subdir('daemon')
endif

if get_option('moss-gtk')
    subdir('moss-gtk')
endif
