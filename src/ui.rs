use crate::translation_service::DictionarySuggestion;

use super::*;
use iced::{
    alignment::Horizontal,
    keyboard,
    pure::*,
    Command,
    Length,
    Subscription,
};
use itertools::Itertools;
use std::{
    fs::DirEntry,
    sync::Arc,
};

#[derive(Clone, Debug, Default)]
pub struct SuggestionPanel {
    pub translator_suggestion: Option<Vec<DictionarySuggestion>>,
    pub project_suggestions: Option<Vec<DictionarySuggestion>>,
    pub global_suggestions: Option<Vec<DictionarySuggestion>>,
}

#[derive(Debug, Clone)]
pub enum AppMode {
    PickingFile {
        current_dir: PathBuf,
    },
    InWorkspace {
        translation_workspace: TranslationWorkspace,
        focused_index: Option<String>,
        suggestions: SuggestionPanel,
    },
}

impl Default for AppMode {
    fn default() -> Self {
        Self::PickingFile {
            current_dir: crate::filesystem::base_directory()
                .expect("failed to find binary's parent dir"),
        }
    }
}
#[derive(Clone, Debug)]
pub struct TlumokState {
    error: Option<String>,
    translation_service: TranslationService,
    app_mode: AppMode,
}

impl TlumokState {
    pub fn new(translation_service: TranslationService) -> Self {
        Self {
            error: Default::default(),
            translation_service,
            app_mode: Default::default(),
        }
    }
    pub fn e(&mut self, error: &eyre::Error) {
        tracing::error!("{error:#?}");
        self.error = Some(format!("{error:#?}"))
    }
}

macro_rules! e {
    ($self:expr, $result:expr) => {{
        match $result {
            Ok(v) => v,
            Err(e) => {
                $self.e(e);
                return Command::none();
            }
        }
    }};
}
#[derive(Debug, Clone, Copy)]
pub enum SuggestionKind {
    Global,
    Machine,
    Project,
}
#[derive(Debug, Clone)]
pub enum Message {
    DocumentSaved(Arc<Result<()>>),
    Save,
    // InitializeTranslationService,
    // TranslationServiceInitialized(Arc<Result<TranslationService>>),
    TranslationInput((String, String)),
    FileSelected(PathBuf),
    NewWorkspaceLoaded(Arc<Result<TranslationWorkspace>>),
    CtrlTab,
    Tab,
    /// user clicked on a translation
    ClickedOn(String),
    RequestedTranslations((SuggestionKind, String)),
    ReceivedTranslations(Arc<(String, SuggestionKind, Result<Vec<DictionarySuggestion>>)>),
    ApplyTranslation(DictionarySuggestion),
}
fn app_title() -> String {
    format!("Tłumok {}", clap::crate_version!())
}

fn dir_entries(path: &Path) -> Result<Vec<DirEntry>> {
    std::fs::read_dir(path)?
        .into_iter()
        .map(|e| e.wrap_err("reading dir entry"))
        .collect()
}

fn file_picker<'a>(current_dir: &'a Path) -> Result<Element<'a, Message>> {
    let named_entry = |path: PathBuf, title: &str| {
        row()
            .push(text(title).width(Length::Fill))
            .push(button("select").on_press(Message::FileSelected(path)))
    };
    let entry = |path: PathBuf| {
        let title = format!(
            "{}",
            path.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        );
        named_entry(path, &title)
    };
    let empty_dir = match current_dir.parent() {
        Some(parent) => column().push(named_entry(parent.to_owned(), "../")),
        None => column(),
    }
    .spacing(8);
    // let empty_dir = column().push(text("..")).push(entry(current_dir.parent));
    let file_picker = dir_entries(&current_dir)?
        .into_iter()
        .fold(empty_dir, |acc, next| acc.push(entry(next.path())));
    Ok(scrollable(file_picker).into())
}

fn or_error<'a, Message>(res: Result<Element<'a, Message>>) -> Element<'a, Message> {
    match res {
        Ok(view) => view,
        Err(e) => text(format!("{e:?}")).color([0.7, 0.0, 0.0]).into(),
    }
}
pub struct WorkspaceManager;
impl WorkspaceManager {
    pub fn view<'a>(
        translation_workspace: &'a TranslationWorkspace,
        focused_index: &'a Option<String>,
        suggestion_panel: &'a SuggestionPanel,
    ) -> Element<'a, Message> {
        let text_cell = || column().width(Length::FillPortion(1));
        let segment_card = |segment: &'a TranslationSegment, key: &'a str| {
            let selected = focused_index.as_ref().map(|i| i == key).unwrap_or_default();
            let color = if selected {
                [0.0, 0.8, 0.0]
            } else {
                [0.0, 0.0, 0.0]
            };
            let translated_part: Element<'a, _> = if selected {
                text_input(
                    "translation",
                    &segment.translated_text.clone(),
                    |new_value| Message::TranslationInput((key.to_string(), new_value)),
                )
                .into()
            } else {
                text(&segment.translated_text).into()
            };
            row()
                .spacing(10)
                .push(text_cell().push(text(&segment.original_text).color(color)))
                .push(text_cell().push(translated_part))
                .push(button("select").on_press(Message::ClickedOn(key.to_string())))
        };
        let translations = translation_workspace
            .segments
            .segments
            .iter()
            .fold(column().spacing(15), |acc, (key, segment)| {
                acc.push(segment_card(segment, key))
            });

        let suggestion_box = |suggestion: &DictionarySuggestion| {
            row()
                .push(text(&suggestion.translated_text))
                .push(button("apply").on_press(Message::ApplyTranslation(suggestion.clone())))
        };
        let suggestions =
            |kind: SuggestionKind, suggestions: &Option<Vec<DictionarySuggestion>>| {
                let title = text(match kind {
                    SuggestionKind::Global => "global",
                    SuggestionKind::Machine => "machine",
                    SuggestionKind::Project => "project",
                })
                .width(Length::Fill)
                .size(30)
                .horizontal_alignment(Horizontal::Center);
                let base = column().align_items(iced::Alignment::Center).push(title); // base_suggestions
                match suggestions.as_ref() {
                    Some(suggestions) => suggestions
                        .iter()
                        .fold(base, |acc, suggestion| acc.push(suggestion_box(suggestion))),
                    None => {
                        if let Some(focused_index) = focused_index.as_ref() {
                            base.push(button("load").on_press(Message::RequestedTranslations((
                                kind,
                                focused_index.clone(),
                            ))))
                        } else {
                            base
                        }
                    }
                }
            };
        let suggestions_panel = column()
            .width(Length::Fill)
            .push(suggestions(
                SuggestionKind::Machine,
                &suggestion_panel.translator_suggestion,
            ))
            .push(suggestions(
                SuggestionKind::Project,
                &suggestion_panel.project_suggestions,
            ))
            .push(suggestions(
                SuggestionKind::Global,
                &suggestion_panel.global_suggestions,
            ));
        row()
            .push(container(scrollable(translations)).width(Length::FillPortion(3)))
            .push(suggestions_panel.width(Length::FillPortion(1)))
            .into()
    }
}
impl Application for TlumokState {
    type Executor = iced::executor::Default;

    type Message = Message;

    type Flags = (TranslationService,);

    fn new((translation_service,): Self::Flags) -> (Self, iced::Command<Self::Message>) {
        (Self::new(translation_service), Command::none())
    }

    fn title(&self) -> String {
        app_title()
    }

    fn subscription(&self) -> Subscription<Message> {
        iced_native::subscription::events_with(|event, status| {
            if let iced_native::event::Status::Captured = status {
                return None;
            }
            match event {
                iced_native::Event::Keyboard(keyboard::Event::KeyPressed {
                    modifiers,
                    key_code,
                }) if key_code == keyboard::KeyCode::Tab => {
                    if modifiers.control() {
                        Some(Message::CtrlTab)
                    } else {
                        Some(Message::Tab)
                    }
                }
                _ => None,
            }
        })
    }
    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        let translation_service = self.translation_service.clone();
        if let Message::NewWorkspaceLoaded(res) = &message {
            match res.as_ref() {
                Ok(translation_workspace) => {
                    self.app_mode = AppMode::InWorkspace {
                        translation_workspace: translation_workspace.clone(),
                        focused_index: translation_workspace
                            .segments
                            .segments
                            .keys()
                            .next()
                            .cloned(),
                        suggestions: Default::default(),
                    }
                }
                Err(e) => self.e(&e),
            }
            return Command::none();
        };
        match &mut self.app_mode {
            AppMode::PickingFile { current_dir } => match message {
                // Message::FileSelected(_) => todo!(),
                Message::FileSelected(dir_entry) => match dir_entry.is_dir() {
                    true => *current_dir = dir_entry,
                    false => {
                        let task =
                            TranslationWorkspace::get_or_create_for_path(dir_entry).map(Arc::new);
                        return Command::perform(task, Message::NewWorkspaceLoaded);
                    }
                },

                _ => {}
            },
            AppMode::InWorkspace {
                translation_workspace,
                focused_index,
                suggestions,
            } => match message {
                Message::TranslationInput((_, new_value)) => {
                    if let Some(focused_index) = focused_index.as_ref() {
                        if let Some(segment) = translation_workspace
                            .segments
                            .segments
                            .get_mut(focused_index)
                        {
                            segment.translated_text = new_value
                        }
                    }
                }
                Message::CtrlTab => {
                    let keys = translation_workspace.segments.segments.keys().collect_vec();
                    if let Some((previous, _)) =
                        keys.iter().zip(keys.iter().skip(1)).find(|(_, current)| {
                            focused_index
                                .as_ref()
                                .map(|f| &&f == current)
                                .unwrap_or_default()
                        })
                    {
                        *focused_index = Some(previous.to_string())
                    }
                }
                Message::Tab => {
                    let keys = translation_workspace.segments.segments.keys().collect_vec();
                    if let Some((previous, _)) =
                        keys.iter().skip(1).zip(keys.iter()).find(|(_, current)| {
                            focused_index
                                .as_ref()
                                .map(|f| &&f == current)
                                .unwrap_or_default()
                        })
                    {
                        *focused_index = Some(previous.to_string())
                    }
                }
                Message::ClickedOn(index) => *focused_index = Some(index),
                Message::RequestedTranslations((kind, index)) => {
                    if let Some(focused_index) = focused_index.as_ref() {
                        let language_pair = {
                            let t = translation_workspace.translation_options;
                            (t.source_language, t.target_language)
                        };
                        if let Some(original_text) =
                            translation_workspace.segments.segments.get(focused_index)
                        {
                            let focused_index = focused_index.clone();
                            let translation_service = translation_service.clone();
                            let dictionary_service = translation_service.dictionary_service.clone();
                            match kind {
                                SuggestionKind::Global => {
                                    let task = dictionary_service.get_global_suggestions(
                                        language_pair,
                                        original_text.original_text.clone(),
                                    );
                                    return Command::perform(task, move |res| {
                                        Message::ReceivedTranslations(Arc::new((
                                            focused_index.clone(),
                                            kind,
                                            res,
                                        )))
                                    });
                                    //         .map(Arc::new);
                                    // return Command::perform(task, Message::NewWorkspaceLoaded);
                                }
                                SuggestionKind::Project => {
                                    let task = dictionary_service.get_project_suggestions(
                                        translation_workspace.original_document.path.clone(),
                                        language_pair,
                                        original_text.original_text.clone(),
                                    );
                                    return Command::perform(task, move |res| {
                                        Message::ReceivedTranslations(Arc::new((
                                            focused_index.clone(),
                                            kind,
                                            res,
                                        )))
                                    });
                                }
                                SuggestionKind::Machine => {
                                    let original_text = original_text.clone();
                                    let task = translation_service.clone().translate_text(
                                        original_text.original_text.clone(),
                                        TlumokTranslationOptions {
                                            source_language: language_pair.0,
                                            target_language: language_pair.1,
                                        },
                                    );
                                    return Command::perform(task, move |res| {
                                        Message::ReceivedTranslations(Arc::new((
                                            focused_index.clone(),
                                            kind,
                                            res.map(|translated_text| {
                                                vec![DictionarySuggestion {
                                                    original_text: original_text
                                                        .original_text
                                                        .clone(),
                                                    translated_text,
                                                    match_type:
                                                        translation_service::MatchType::Exact,
                                                }]
                                            }),
                                        )))
                                    });
                                    // },
                                }
                            }
                        }
                    }
                }
                Message::ReceivedTranslations(event) => {
                    let (key, kind, new_suggestions) = event.as_ref();
                    if let Some(focused_index) = focused_index.as_ref() {
                        if key == focused_index {
                            match new_suggestions {
                                Ok(new_suggestions) => match kind {
                                    SuggestionKind::Global => {
                                        suggestions.global_suggestions =
                                            Some(new_suggestions.clone())
                                    }
                                    SuggestionKind::Machine => {
                                        suggestions.translator_suggestion =
                                            Some(new_suggestions.clone())
                                    }
                                    SuggestionKind::Project => {
                                        suggestions.project_suggestions =
                                            Some(new_suggestions.clone())
                                    }
                                },
                                Err(e) => tracing::error!("{e:?}"),
                            }
                            // match kind {
                            //     SuggestionKind::Global => match new_suggestions {
                            //         Ok(v) => suggestions.global_suggestions = Some(v.clone()),
                            //         Err(e) => ,
                            //     },
                            //     SuggestionKind::Machine => match new_suggestions {
                            //         Ok(v) => suggestions.translator_suggestion = Some(v.clone()),

                            //         Err(e) => ,
                            //     },
                            //     SuggestionKind::Project => match new_suggestions {
                            //         Ok(v) => suggestions.project_suggestions = Some(v.clone()),
                            //         Err(e) => tracing::error!("{e:?}"),
                            //     },
                            // }
                        }
                    }
                } // _ => {}
                Message::FileSelected(_) => todo!(),
                Message::NewWorkspaceLoaded(_) => todo!(),
                Message::ApplyTranslation(dictionary_suggestion) => {
                    if let Some(focused_index) = focused_index.as_ref() {
                        if let Some(segment) = translation_workspace
                            .segments
                            .segments
                            .get_mut(focused_index)
                        {
                            segment.translated_text = dictionary_suggestion.translated_text.clone()
                        }
                    }
                }
                Message::Save => {
                    return Command::perform(
                        translation_workspace
                            .clone()
                            .save_translated_document()
                            .map(Arc::new),
                        Message::DocumentSaved,
                    )
                }
                Message::DocumentSaved(res) => match res.as_ref() {
                    Ok(_) => {}
                    Err(e) => self.e(&e),
                },
            },
        }
        // match message {
        //     Message::FileSelected(dir_entry) => match dir_entry.is_dir() {
        //         true => {
        //             if let AppMode::PickingFile { current_dir } = &mut self.app_mode {
        //                 *current_dir = dir_entry
        //             }
        //         }
        //         false => {
        //             let task =
        //                 TranslationWorkspace::get_or_create_for_path(dir_entry).map(Arc::new);
        //             return Command::perform(task, Message::NewWorkspaceLoaded);
        //         }
        //     },
        //     Message::NewWorkspaceLoaded(res) => match res.as_ref() {

        //     },
        //     Message::TranslationInput((key, new_value)) => {
        //         if let AppMode::InWorkspace {
        //             translation_workspace,
        //             focused_index,
        //             suggestions,
        //         } = &mut self.app_mode
        //         {

        //         }
        //     }
        //     Message::ClickedOn(index) => {
        //         if let AppMode::InWorkspace {
        //             translation_workspace,
        //             focused_index,
        //             suggestions,
        //         } = &mut self.app_mode
        //         {

        //         }
        //     }
        //     Message::CtrlTab => {
        //         if let AppMode::InWorkspace {
        //             translation_workspace,
        //             focused_index,
        //             suggestions,
        //         } = &mut self.app_mode
        //         {

        //         }
        //     }
        //     Message::Tab => {
        //         if let AppMode::InWorkspace {
        //             translation_workspace,
        //             focused_index,
        //             suggestions,
        //         } = &mut self.app_mode
        //         {

        //         }
        //     }
        //     Message::RequestedTranslations((kind, index)) => match kind {
        //         SuggestionKind::Global => ,
        //         SuggestionKind::Machine => todo!(),
        //         SuggestionKind::Project => todo!(),
        //     },
        // }
        Command::none()
    }

    fn view(&self) -> iced::pure::Element<'_, Self::Message> {
        let main_view = column()
            .align_items(iced::Alignment::Center)
            .width(Length::Fill)
            .push(match &self.app_mode {
                AppMode::PickingFile { current_dir } => or_error(file_picker(&current_dir)),
                AppMode::InWorkspace {
                    translation_workspace,
                    focused_index,
                    suggestions,
                } => WorkspaceManager::view(translation_workspace, focused_index, suggestions),
            });
        let errors = match &self.error {
            Some(e) => column().push(text(format!("{e:#?}")).color([0.7, 0., 0.])),
            None => column(),
        };
        let navbar = text(app_title())
            .width(Length::Fill)
            .size(30)
            .horizontal_alignment(Horizontal::Center);
        let content = column()
            .width(Length::Fill)
            .spacing(10)
            .push(navbar)
            .push(main_view)
            .push(errors.width(Length::Fill).height(Length::Shrink));
        let app = container(content)
            .width(Length::Fill)
            .center_x()
            .center_y()
            .padding(30);
        app.into()
    }
}
pub mod style {
    pub struct Title;
}
