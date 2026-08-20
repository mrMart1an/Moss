use std::path::PathBuf;

use argparse::{ArgumentParser, Print, StoreOption};

pub struct ArgsOptions {
    pub config_file_path: Option<PathBuf>,
}

impl ArgsOptions {
    pub fn parse() -> Self {
        let mut options = ArgsOptions::default();

        {
            let mut parser = ArgumentParser::new();

            // Configuration file path
            parser.refer(&mut options.config_file_path).add_option(
                &["-c", "--config"],
                StoreOption,
                "The file path of the configuration file",
            );

            // Show daemon version
            parser.add_option(
                &["-V", "--version"],
                Print(env!("CARGO_PKG_VERSION").to_string()),
                "Show the daemon version"
            );

            parser.parse_args_or_exit();
        }

        options
    }
}

impl Default for ArgsOptions {
    fn default() -> Self {
        Self { 
            config_file_path: None,
        }
    }
}
