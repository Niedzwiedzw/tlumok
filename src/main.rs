use std::path::{
    Path,
    PathBuf,
};

use eyre::{
    Result,
    WrapErr,
};

use tracing_subscriber::{
    fmt,
    prelude::__tracing_subscriber_SubscriberExt,
    EnvFilter,
};

pub mod ui {

    use iced::{
        pure::*,
        Command,
    };
    #[derive(Default, Debug, Clone)]
    pub struct TlumokState {}
    #[derive(Debug, Clone)]
    pub enum Message {}
    fn app_title() -> String {
        format!("Tłumok {}", clap::crate_version!())
    }
    impl Application for TlumokState {
        type Executor = iced::executor::Default;

        type Message = Message;

        type Flags = ();

        fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
            (Self::default(), Command::none())
        }

        fn title(&self) -> String {
            app_title()
        }

        fn update(&mut self, _message: Self::Message) -> iced::Command<Self::Message> {
            Command::none()
        }

        fn view(&self) -> iced::pure::Element<'_, Self::Message> {
            let content = column().push(text(app_title()));
            container(content).into()
        }
    }
}
pub mod filesystem {
    use std::path::PathBuf;

    use super::*;
    pub fn base_directory() -> Result<PathBuf> {
        let base_dir = std::env::current_exe()
            .wrap_err("Nie znaleziono folderu w którym znajduje się aplikacja")?
            .parent()
            .ok_or_else(|| eyre::eyre!("aplikacja musi być w jakimś folderze"))?
            .to_owned();
        if !base_dir.exists() {
            std::fs::create_dir_all(&base_dir).context("tworzenie folderu dla aplikacji")?;
        }
        Ok(base_dir)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TlumokConfig {
    pub deepl_api_key: String,
}

impl TlumokConfig {
    pub const DEFAULT_CONFIG_FILENAME: &'static str = "tlumok-settings.yaml";
    pub fn load_default() -> Result<Self> {
        Self::load(&filesystem::base_directory()?.join(Self::DEFAULT_CONFIG_FILENAME))
    }
    pub fn load(path: &Path) -> Result<Self> {
        std::fs::read_to_string(path)
            .wrap_err_with(|| format!("reading [{path:?}]"))
            .and_then(|content| toml::from_str(&content).wrap_err("parsing config"))
    }
}
use clap::{
    Parser,
    Subcommand,
};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    // /// Optional name to operate on
    // name: Option<String>,

    // /// Sets a custom config file
    // #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    // config: Option<PathBuf>,

    // /// Turn debugging information on
    // #[clap(short, long, parse(from_occurrences))]
    // debug: usize,
    /// translated file path
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    file: PathBuf,
    /// command
    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    // /// does testing things
    // Test {
    //     /// lists test values
    //     #[clap(short, long)]
    //     list: bool,
    // },
}

#[tokio::main]
async fn main() -> Result<()> {
    let logs_dir = filesystem::base_directory()?.join("logs");
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "log.txt");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::TRACE.into()))
        .with(fmt::Layer::new().with_writer(std::io::stdout))
        .with(
            fmt::Layer::new()
                .compact()
                .with_ansi(false)
                .with_writer(non_blocking),
        );
    tracing::subscriber::set_global_default(subscriber)
        .context("Unable to set a global subscriber")?;
    // <ui::TlumokState as iced::pure::Application>::run(iced::Settings::default())
    //     .wrap_err("running app")?;

    Ok(())
}
