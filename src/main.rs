#![feature(path_try_exists)]
use eyre::{
    Result,
    WrapErr,
};
use futures::StreamExt;
use futures::TryStreamExt;
use std::path::{
    Path,
    PathBuf,
};
use tracing_subscriber::{
    fmt,
    prelude::__tracing_subscriber_SubscriberExt,
    EnvFilter,
};

use futures::FutureExt;
pub mod key_value_cache;

pub mod ui;
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
    pub fn dictionaries_directory() -> Result<PathBuf> {
        let db_path = crate::filesystem::base_directory()?.join("dictionaries");
        Ok(db_path)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct TlumokConfig {
    pub deepl_api_key: String,
}

impl TlumokConfig {
    pub const DEFAULT_CONFIG_FILENAME: &'static str = "tlumok-settings.toml";
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
    #[clap(subcommand)]
    command: Option<Commands>,
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
    GenerateDefaultTlumokConfig, // /// does testing things
    /// creates a new file of the original format, with translations applied
    ApplyTranslations {
        /// translated file path
        #[clap(short, long, parse(from_os_str), value_name = "FILE")]
        file: PathBuf,
    },
}
use serde::{
    Deserialize,
    Serialize,
};
pub static NOT_TRANSLATED_MARKER: &'static str = "TODO!!!";
pub mod translation_service {
    use std::sync::Arc;

    use super::Result;
    use super::*;
    use deepl_api::*;
    use itertools::Itertools;
    use tokio::sync::{
        Mutex,
        RwLock,
    };

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy)]
    pub enum Language {
        Polish,
        English,
    }
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
    pub struct TlumokTranslationOptions {
        pub source_language: Language,
        pub target_language: Language,
    }
    pub type LanguagePair = (Language, Language);
    impl std::fmt::Display for Language {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.to_deepl_language_static())
        }
    }
    impl Default for TlumokTranslationOptions {
        fn default() -> Self {
            Self {
                source_language: Language::English,
                target_language: Language::Polish,
            }
        }
    }

    impl Language {
        pub fn to_deepl_language_static(self) -> &'static str {
            match self {
                Language::Polish => "PL",
                Language::English => "EN",
            }
        }
        pub fn to_deepl_language(self) -> String {
            self.to_deepl_language_static().to_string()
        }

        pub fn deepl_language_opt(self) -> Option<String> {
            Some(self.to_deepl_language())
        }
    }

    impl TlumokTranslationOptions {
        pub fn default_translatable_text_list(&self) -> TranslatableTextList {
            TranslatableTextList {
                source_language: self.source_language.deepl_language_opt(),
                target_language: self.target_language.to_deepl_language(),
                texts: vec![],
            }
        }

        pub fn translatable_text_list(&self, text: &str) -> TranslatableTextList {
            TranslatableTextList {
                texts: vec![text.to_string()],
                ..self.default_translatable_text_list()
            }
        }
    }

    pub type Translation = (String, Vec<String>);
    pub type TranslationCache = CacheFor<Translation>;
    use crate::key_value_cache::cache_service::{
        dictionary_at_path,
        CacheFor,
    };
    #[derive(Debug, Clone, Default)]
    pub struct DictionaryService(Arc<RwLock<()>>);

    impl DictionaryService {
        pub async fn save_translation(
            self,
            original_document_path: PathBuf,
            language_pair: LanguagePair,
            original_text: String,
            translated_text: String,
        ) -> Result<()> {
            let _guard = self.0.write().await;
            let cache = tokio::task::block_in_place(|| {
                crate::key_value_cache::cache_service::project_dictionary(
                    &original_document_path,
                    language_pair,
                )
            })
            .wrap_err_with(|| format!("fetching db based on project [{original_document_path:?}] and languages [{language_pair:?}]"))?;
            let current = cache.get(original_text.clone()).await?.unwrap_or_default();
            let updated = current
                .into_iter()
                .chain(std::iter::once(translated_text))
                .collect();
            cache.insert(original_text, updated).await?;
            Ok(())
        }
        async fn get_suggestions_from_db(
            self,
            db: TranslationCache,
            original_text: String,
        ) -> Result<Vec<DictionarySuggestion>> {
            let _guard = self.0.read().await;
            let mut suggestions = vec![];

            if let Some(exact) = db.get(original_text.clone()).await? {
                for translated_text in exact.into_iter() {
                    suggestions.push(DictionarySuggestion {
                        original_text: original_text.clone(),
                        translated_text,
                        match_type: MatchType::Exact,
                    });
                }
            }
            Ok(suggestions)
        }
        pub async fn get_project_suggestions(
            self,
            original_document_path: PathBuf,
            language_pair: LanguagePair,
            original_text: String,
        ) -> Result<Vec<DictionarySuggestion>> {
            let _guard = (&self.0).read().await;

            let cache = tokio::task::block_in_place(|| {
                crate::key_value_cache::cache_service::project_dictionary(
                    &original_document_path,
                    language_pair,
                )
            })
            .wrap_err_with(|| format!("fetching db based on project [{original_document_path:?}] and languages [{language_pair:?}]"))?;
            self.clone()
                .get_suggestions_from_db(cache, original_text)
                .await
        }
        pub async fn get_global_suggestions(
            self,
            language_pair: LanguagePair,
            original_text: String,
        ) -> Result<Vec<DictionarySuggestion>> {
            let lang_dir =
                crate::key_value_cache::cache_service::language_pair_db_key(language_pair)?;
            let valid_dictionary_dirs = tokio::task::block_in_place(|| -> Result<_> {
                let dictionary_dirs =
                    std::fs::read_dir(&lang_dir).wrap_err("reading all project dictionaries")?;
                let valid = dictionary_dirs
                    .into_iter()
                    .filter_map(|d| d.ok())
                    .map(|d| d.path())
                    .filter(|d| d.is_dir())
                    .collect_vec();
                Ok(valid)
            })?;
            let dictionaries = valid_dictionary_dirs
                .into_iter()
                .filter_map(|path| dictionary_at_path(path).ok());
            let suggestions: Vec<Vec<_>> = futures::stream::iter(dictionaries)
                .map(|db| {
                    self.clone()
                        .get_suggestions_from_db(db, original_text.clone())
                })
                .buffer_unordered(10)
                .try_collect()
                .await?;
            let mut out = vec![];
            let mut uniques = std::collections::HashSet::new();
            for suggestion in suggestions.into_iter().flatten() {
                if uniques.contains(&suggestion.translated_text) {
                    continue;
                }
                uniques.insert(suggestion.translated_text.clone());
                out.push(suggestion);
            }
            Ok(out)
        }
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MatchType {
        Exact,
        PartialPercent(u32),
    }
    #[derive(Debug, Clone)]
    pub struct DictionarySuggestion {
        pub original_text: String,
        pub translated_text: String,
        pub match_type: MatchType,
    }
    // #[derive(Debug, Clone)]
    // pub struct DictionarySuggestions {
    //     pub project_suggestions: Vec<DictionarySuggestion>,
    //     pub global_suggestions: Vec<DictionarySuggestion>,
    // }

    #[derive(Clone)]
    pub struct TranslationService {
        pub deepl_client: Arc<Mutex<DeepL>>,
        /// hidden behind a RwLock to prevent data-races
        pub dictionary_service: DictionaryService,
    }
    impl std::fmt::Debug for TranslationService {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TranslationService")
        }
    }

    impl TranslationService {
        pub async fn new(deepl_api_key: String) -> Result<Self> {
            let deepl_client = DeepL::new(deepl_api_key);
            tracing::info!(
                "{:#?}",
                deepl_client
                    .usage_information()
                    .await
                    .map_err(|e| eyre::eyre!("{e:?}"))
                    .wrap_err("connecting to deepl api")?
            );
            let deepl_client = Arc::new(Mutex::new(deepl_client));
            Ok(Self {
                deepl_client,
                dictionary_service: Default::default(),
            })
        }
    }

    impl TranslationService {
        #[tracing::instrument(skip(self), level = "info")]
        pub async fn translate_text(
            self,
            text: String,
            translation_options: TlumokTranslationOptions,
        ) -> Result<String> {
            let translatable_text_list = translation_options.translatable_text_list(&text);
            let deepl_client = self.deepl_client.lock().await;
            let translated = deepl_client
                .translate(None, translatable_text_list)
                .await
                .map_err(|e| eyre::eyre!("{e:?}"))
                .wrap_err_with(|| format!("getting translation info from deepl"))?;

            let translated = translated
                .get(0)
                .ok_or_else(|| eyre::eyre!("parsing deepl response"))?;
            let translated = translated.text.clone();
            tracing::info!("translated: \n[{text}]\n->\n[{translated}]");
            Ok(translated)
        }
        pub async fn translate_segment(
            self,
            segment: TranslationSegment,
            translation_options: TlumokTranslationOptions,
        ) -> Result<TranslationSegment> {
            let TranslationSegment {
                original_text,
                confirmed,
                original_document_slice,
                ..
            } = segment.clone();
            if confirmed.is_some() {
                Ok(segment)
            } else {
                let translated_text = self
                    .translate_text(original_text.clone(), translation_options)
                    .await?;
                Ok(TranslationSegment {
                    original_text,
                    translated_text,
                    original_document_slice,
                    confirmed,
                })
            }
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
    pub confirmed: Option<String>,
    pub original_document_slice: OriginalDocumentSlice,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileFormat {
    Txt,
}

impl std::fmt::Display for FileFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Txt => "txt",
            }
        )
    }
}
use indexmap::IndexMap;
use translation_service::{
    TlumokTranslationOptions,
    TranslationService,
};
type TranslationSegmentMap = IndexMap<String, TranslationSegment>;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationSegments {
    pub segments: TranslationSegmentMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationWorkspace {
    pub tlumok_version: String,
    pub original_document: OriginalDocument,
    pub translation_options: TlumokTranslationOptions,
    pub segments: TranslationSegments,
}
impl TranslationSegments {
    pub async fn translate(
        self,
        translation_service: &translation_service::TranslationService,
        translation_options: TlumokTranslationOptions,
    ) -> Result<Self> {
        let segments = futures::stream::iter(self.segments)
            .map(|(index, segment)| {
                translation_service
                    .clone()
                    .translate_segment(segment, translation_options)
                    .map(|result| result.map(|translated| (index, translated)))
            })
            .buffer_unordered(4)
            .try_collect()
            .await
            .wrap_err("translating document segments")?;
        Ok(Self { segments })
    }
    pub async fn generate_from(text: &str) -> Result<Self> {
        use unicode_segmentation::UnicodeSegmentation;
        tracing::info!("generating segments");
        let empty_segments: Vec<_> = text
            .split_sentence_bound_indices()
            .into_iter()
            .map(|(start, sentence)| TranslationSegment {
                original_text: sentence.to_string(),
                translated_text: NOT_TRANSLATED_MARKER.to_string(),
                confirmed: None,
                original_document_slice: OriginalDocumentSlice {
                    start,
                    len: sentence.len(),
                },
            })
            .collect();
        tracing::info!("translating segments");

        let segments = empty_segments
            .into_iter()
            .enumerate()
            .map(|(index, segment)| (format!("segment_{index}"), segment))
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

pub type AppTime = chrono::NaiveDateTime;
pub fn now() -> AppTime {
    chrono::Local::now().naive_local()
}

static FILE_SAFE_DATETIME: &'static str = "%Y-%m-%d--%H-%M-%S";
impl TranslationWorkspace {
    pub async fn save_translated_document(self) -> Result<()> {
        let document_path = self.original_document.path.clone();
        let original_extension = self.original_document.file_format.clone();
        let translated_document = self.create_translated_document().await?;
        let now_pretty = now().format(FILE_SAFE_DATETIME);
        let translated_document_path = document_path.with_extension(format!(
            "tlumok-translated.{now_pretty}.{original_extension}"
        ));
        tokio::fs::write(&translated_document_path, &translated_document)
            .await
            .wrap_err_with(|| {
                format!("saving translated document to {translated_document_path:?}")
            })?;
        Ok(())
    }
    pub async fn create_translated_document(self) -> Result<String> {
        let Self {
            original_document: OriginalDocument { path, .. },
            segments: TranslationSegments { segments },
            ..
        } = self.validated()?;
        let mut translated_content = tokio::fs::read_to_string(&path)
            .await
            .wrap_err_with(|| format!("reading original document at [{path:?}]"))?;
        for (
            _,
            TranslationSegment {
                original_text,
                translated_text,
                original_document_slice: OriginalDocumentSlice { start, len },
                ..
            },
        ) in segments.into_iter().rev()
        {
            let range = start..(start + len);
            let document_content = &translated_content[range.clone()];
            if document_content != original_text {
                eyre::bail!("original text from workspace file did not match actual contents of document\ndocument content: [{document_content}]\n according to workspace document: [{original_text}]");
            }
            translated_content.replace_range(range, &translated_text);
        }

        Ok(translated_content)
    }
    pub fn validated(self) -> Result<Self> {
        let validated = &self;
        // if let Some((index, segment)) = validated
        //     .segments
        //     .segments
        //     .iter()
        //     .find(|(_, segment)| !segment.translated)
        // {
        //     eyre::bail!("segment [{index}] is not translated\n\n{segment:#?}");
        // }
        if let Some((index, segment)) = validated
            .segments
            .segments
            .iter()
            .find(|(_, segment)| !segment.confirmed.is_none())
        {
            eyre::bail!("segment [{index}] is not checked\n\n{segment:#?}");
        }

        Ok(self)
    }
    pub async fn translate(self, translation_service: &TranslationService) -> Result<Self> {
        let translation_options = self.translation_options;
        Ok(Self {
            segments: self
                .segments
                .translate(translation_service, translation_options)
                .await?,
            ..self
        })
    }
    pub fn default_path_for_document(OriginalDocument { path, .. }: &OriginalDocument) -> PathBuf {
        path.with_extension("tlumok-workspace.toml")
    }

    #[tracing::instrument]
    pub async fn load(path: &Path) -> Result<Self> {
        let content = tokio::fs::read_to_string(path)
            .await
            .wrap_err_with(|| format!("reading workspace from {path:?}"))?;
        let workspace = toml::from_str(&content)
            .wrap_err_with(|| format!("reading contents of [{path:?}] file"))?;
        tracing::info!("loaded workspace to [{path:?}]");

        Ok(workspace)
    }

    #[tracing::instrument(skip(self))]
    pub async fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self).wrap_err("serializing workspace")?;
        tokio::fs::write(path, &content)
            .await
            .wrap_err_with(|| format!("writing workspace to [{path:?}]"))?;
        tracing::info!("saved workspace to [{path:?}]");
        Ok(())
    }

    pub async fn for_document(original_document: OriginalDocument) -> Result<Self> {
        let segments = TranslationSegments::for_document(&original_document)
            .await
            .context("generating translation segments")?;
        Ok(Self {
            original_document,
            segments,
            tlumok_version: clap::crate_version!().to_string(),
            translation_options: Default::default(),
        })
    }
    pub async fn get_or_create_for_document(original_document: OriginalDocument) -> Result<Self> {
        let default_path = Self::default_path_for_document(&original_document);
        let translation_workspace = if default_path.exists() {
            Self::load(&default_path).await?
        } else {
            Self::for_document(original_document).await?
        };
        translation_workspace.save(&default_path).await?;
        Ok(translation_workspace)
    }

    pub async fn get_or_create_for_path(path: PathBuf) -> Result<Self> {
        let original_document = OriginalDocument::from_file(&path)?;
        Self::get_or_create_for_document(original_document).await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let logs_dir = filesystem::base_directory()?.join("logs");
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "log.txt");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
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
        Some(command) => match command {
            Commands::GenerateDefaultTlumokConfig => {
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
            Commands::Translate { file } => {
                let file = file.canonicalize()?;
                Path::try_exists(&file).wrap_err("opening document for translation")?;
                let original_document = OriginalDocument::from_file(&file)
                    .wrap_err_with(|| format!("opening original document {file:?}"))?;
                let default_path =
                    TranslationWorkspace::default_path_for_document(&original_document);
                default_path
                    .try_exists()
                    .wrap_err("translation workspace does not exist")?;
                let TlumokConfig { deepl_api_key } = TlumokConfig::load_default()?;
                let translation_service = TranslationService::new(deepl_api_key).await?;

                let translation_workspace = TranslationWorkspace::load(&default_path).await?;
                let translation_workspace = translation_workspace
                    .translate(&translation_service)
                    .await?;
                translation_workspace.save(&default_path).await?;
            }
            Commands::InitializeTranslationWorkspace { file } => {
                let file = file.canonicalize()?;
                Path::try_exists(&file).wrap_err("opening document for translation")?;
                let original_document = OriginalDocument::from_file(&file)
                    .wrap_err_with(|| format!("opening original document {file:?}"))?;
                let default_path =
                    TranslationWorkspace::default_path_for_document(&original_document);
                if default_path.exists() {
                    tracing::error!("[{default_path:?}] already exists");
                } else {
                    let translation_workspace =
                        TranslationWorkspace::for_document(original_document)
                            .await
                            .context("creating workspace for [{file:?}]")?;
                    translation_workspace.save(&default_path).await?;
                    tracing::info!("new workspace generated at [{default_path:?}]");
                }
            }
            Commands::ApplyTranslations { file } => {
                let file = file.canonicalize()?;

                Path::try_exists(&file).wrap_err("opening document for translation")?;

                let original_document = OriginalDocument::from_file(&file)
                    .wrap_err_with(|| format!("opening original document {file:?}"))?;
                let default_path =
                    TranslationWorkspace::default_path_for_document(&original_document);
                let translation_workspace = TranslationWorkspace::load(&default_path).await?;
                let translation_workspace = translation_workspace.validated()?;
                // let translated_document = translation_workspace.create_translated_document().await?;
                translation_workspace.save_translated_document().await?;
            }
        },
        None => {
            tracing::info!("getting deepl api key");
            let TlumokConfig { deepl_api_key } = TlumokConfig::load_default()?;
            tracing::info!("connecting to deepl api and setting up dictionary databases");
            let translation_service = TranslationService::new(deepl_api_key).await?;
            tracing::info!("starting graphical interface");
            <ui::TlumokState as iced::pure::Application>::run(iced::Settings::with_flags((
                translation_service,
            )))?;
        }
    }
    Ok(())
}
