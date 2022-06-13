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
#[derive(Debug, Clone)]
pub enum AppMode {
    PickingFile {
        current_dir: PathBuf,
    },
    InWorkspace {
        translation_workspace: TranslationWorkspace,
        focused_index: Option<String>,
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
#[derive(Default, Debug, Clone)]
pub struct TlumokState {
    error: Option<Arc<eyre::Error>>,
    app_mode: AppMode,
}

impl TlumokState {
    pub fn e(&mut self, error: eyre::Error) {
        tracing::error!("{error:#?}");
        self.error = Some(Arc::new(error))
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

#[derive(Debug, Clone)]
pub enum Message {
    TranslationInput((String, String)),
    FileSelected(PathBuf),
    NewWorkspaceLoaded(Arc<Result<TranslationWorkspace>>),
    CtrlTab,
    Tab,
    /// user clicked on a translation
    ClickedOn(String),
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
        focused_index: &Option<String>,
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
        scrollable(
            container(
                translation_workspace
                    .segments
                    .segments
                    .iter()
                    .fold(column().spacing(15), |acc, (key, segment)| {
                        acc.push(segment_card(segment, key))
                    }),
            )
            .padding(15),
        )
        .into()
    }
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
        match message {
            Message::FileSelected(dir_entry) => match dir_entry.is_dir() {
                true => {
                    if let AppMode::PickingFile { current_dir } = &mut self.app_mode {
                        *current_dir = dir_entry
                    }
                }
                false => {
                    let task =
                        TranslationWorkspace::get_or_create_for_path(dir_entry).map(Arc::new);
                    return Command::perform(task, Message::NewWorkspaceLoaded);
                }
            },
            Message::NewWorkspaceLoaded(res) => match res.as_ref() {
                Ok(translation_workspace) => {
                    self.app_mode = AppMode::InWorkspace {
                        translation_workspace: translation_workspace.clone(),
                        focused_index: translation_workspace
                            .segments
                            .segments
                            .keys()
                            .next()
                            .cloned(),
                    }
                }
                Err(e) => self.e(eyre::eyre!("{e:#?}")),
            },
            Message::TranslationInput((key, new_value)) => {
                if let AppMode::InWorkspace {
                    translation_workspace,
                    focused_index,
                } = &mut self.app_mode
                {
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
            }
            Message::ClickedOn(index) => {
                if let AppMode::InWorkspace {
                    translation_workspace,
                    focused_index,
                } = &mut self.app_mode
                {
                    *focused_index = Some(index)
                }
            }
            Message::CtrlTab => {
                if let AppMode::InWorkspace {
                    translation_workspace,
                    focused_index,
                } = &mut self.app_mode
                {
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
            }
            Message::Tab => {
                if let AppMode::InWorkspace {
                    translation_workspace,
                    focused_index,
                } = &mut self.app_mode
                {
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
            }
        }
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
                } => WorkspaceManager::view(translation_workspace, focused_index),
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
            .max_width(1200)
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
