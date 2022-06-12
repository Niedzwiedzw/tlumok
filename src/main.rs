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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct TlumokConfig {
    pub deepl_api_key: String,
}

impl TlumokConfig {
    pub const DEFAULT_CONFIG_FILENAME: &'static str = "tlumok-settings.yaml";
    pub fn default_config_path() -> Result<PathBuf> {
        Ok(filesystem::base_directory()?.join(Self::DEFAULT_CONFIG_FILENAME))
    }
    pub fn load_default() -> Result<Self> {
        Self::load(&Self::default_config_path()?)
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
    /// command
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// initializes workspace for a given document
    InitializeTranslationWorkspace {
        #[clap(short, long, parse(from_os_str), value_name = "FILE")]
        file: PathBuf,
    },
    /// uses the generated workspace to perform a translation on a target file
    Translate {
        /// translated file path
        #[clap(short, long, parse(from_os_str), value_name = "FILE")]
        file: PathBuf,
    },
    /// generates default config
    GenerateDefaultConfig, // /// does testing things
                           // Test {
                           //     /// lists test values
                           //     #[clap(short, long)]
                           //     list: bool,
                           // },
}
use serde::{
    Deserialize,
    Serialize,
};

pub mod translation_service {
    use super::*;
    use deepl_api::*;

    pub struct TranslationService {
        pub deepl_client: DeepL,
    }

    impl TranslationService {
        pub fn new(deepl_api_key: String) -> Self {
            let deepl_client = DeepL::new(deepl_api_key);

            Self { deepl_client }
        }
    }

    impl TranslationService {
        pub async fn translate_text(text: &str) -> Result<String> {
            let task = TranslatableTextList {};
        }
    }
}

/// this represents the original file that is being translated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OriginalDocument {
    /// original file path
    pub path: PathBuf,
    pub file_format: FileFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OriginalDocumentSlice {
    pub start: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationSegment {
    pub original_text: String,
    pub translated_text: String,
    pub checked: bool,
    original_document_slice: OriginalDocumentSlice,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileFormat {
    Txt,
}
use indexmap::IndexMap;
type TranslationSegmentMap = IndexMap<String, TranslationSegment>;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationSegments {
    pub segments: TranslationSegmentMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationWorkspace {
    pub tlumok_version: String,
    pub original_document: OriginalDocument,
    pub segments: TranslationSegments,
}

impl TranslationSegments {
    pub async fn generate_from(text: &str) -> Result<Self> {
        use unicode_segmentation::UnicodeSegmentation;
        let segments: TranslationSegmentMap = text
            .split_sentence_bound_indices()
            .into_iter()
            .enumerate()
            .map(|(index, (start, sentence))| {
                (
                    format!("segment_{index}"),
                    TranslationSegment {
                        original_text: sentence.to_string(),
                        translated_text: sentence.to_string(), // here the automatic translation should be performed
                        checked: false,
                        original_document_slice: OriginalDocumentSlice {
                            start,
                            len: sentence.len(),
                        },
                    },
                )
            })
            .collect();
        Ok(Self { segments })
    }

    pub async fn for_document(
        OriginalDocument { path, file_format }: &OriginalDocument,
    ) -> Result<Self> {
        match file_format {
            FileFormat::Txt => {
                let content = tokio::fs::read_to_string(path)
                    .await
                    .wrap_err_with(|| format!("opening {path:?} for segment generation"))?;
                Ok(Self::generate_from(&content)
                    .await
                    .context("generating default translation segments")?)
            }
        }
    }
}
impl OriginalDocument {
    pub fn from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            eyre::bail!("{path:?} does not exist")
        }
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_string());

        let file_format = match extension.as_ref().map(|v| v.as_str()) {
            Some("txt") => FileFormat::Txt,
            e => eyre::bail!("bad extension :: {e:?}"),
        };
        Ok(Self {
            path: path.to_owned(),
            file_format,
        })
    }
}

impl TranslationWorkspace {
    pub fn default_path_for_document(OriginalDocument { path, .. }: &OriginalDocument) -> PathBuf {
        path.with_extension("tlumok-workspace.toml")
    }

    pub async fn for_document(original_document: OriginalDocument) -> Result<Self> {
        let segments = TranslationSegments::for_document(&original_document)
            .await
            .context("generating translation segments")?;
        Ok(Self {
            original_document,
            segments,
            tlumok_version: clap::crate_version!().to_string(),
        })
    }
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
    let Cli { command } = Cli::parse();
    match command {
        Commands::GenerateDefaultConfig => {
            let config = TlumokConfig::default();
            let target_path = TlumokConfig::default_config_path()?;
            if target_path.exists() {
                tracing::error!("target path {target_path:?} already exists");
                return Ok(());
            }
            std::fs::write(
                target_path,
                toml::to_string_pretty(&config).wrap_err("serializing default config")?,
            )
            .wrap_err("writing default config")?;
        }
        Commands::Translate { file } => todo!(),
        Commands::InitializeTranslationWorkspace { file } => {
            let original_document = OriginalDocument::from_file(&file)
                .wrap_err_with(|| format!("opening original document {file:?}"))?;
            let default_path = TranslationWorkspace::default_path_for_document(&original_document);
            if default_path.exists() {
                tracing::error!("[{default_path:?}] already exists");
            } else {
                let translation_workspace = TranslationWorkspace::for_document(original_document)
                    .await
                    .context("creating workspace for [{file:?}]")?;
                let content =
                    toml::to_string_pretty(&translation_workspace).wrap_err_with(|| {
                        format!("serializing translation workspace for [{file:?}]")
                    })?;
                tokio::fs::write(&default_path, content)
                    .await
                    .wrap_err_with(|| format!("saving a default workspace for {file:?}"))?;
                tracing::info!("new workspace generated at [{default_path:?}]");
            }
        }
    };
    let TlumokConfig { deepl_api_key } = TlumokConfig::load_default()?;
    // <ui::TlumokState as iced::pure::Application>::run(iced::Settings::default())
    //     .wrap_err("running app")?;

    Ok(())
}
