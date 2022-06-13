use std::{
    fs::DirEntry,
    sync::Arc,
};

use super::*;
use iced::{
    alignment::Horizontal,
    pure::*,
    Command,
    Length,
};
#[derive(Debug, Clone)]
pub enum AppMode {
    PickingFile {
        current_dir: PathBuf,
    },
    InWorkspace {
        translation_workspace: TranslationWorkspace,
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
    FileSelected(PathBuf),
    NewWorkspaceLoaded(Arc<Result<TranslationWorkspace>>),
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
    let entry = |path: PathBuf| {
        let title = format!(
            "{}",
            path.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        );
        row()
            .push(text(title).width(Length::Fill))
            .push(button("select").on_press(Message::FileSelected(path)))
    };
    let empty_dir = match current_dir.parent() {
        Some(parent) => column().push(row().push(text("../")).push(entry(parent.to_owned()))),

        None => column(),
    };
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
    pub fn view<'a>(translation_workspace: &'a TranslationWorkspace) -> Element<'a, Message> {
        let segment_card = |segment: &'a TranslationSegment| {
            row()
                .push(text(&segment.original_text))
                .push(text(&segment.translated_text))
        };
        scrollable(
            translation_workspace
                .segments
                .segments
                .iter()
                .fold(column(), |acc, (_, segment)| {
                    acc.push(segment_card(segment))
                }),
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
                    }
                }
                Err(e) => self.e(eyre::eyre!("{e:#?}")),
            },
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
                } => WorkspaceManager::view(translation_workspace),
                // Some(translation_workspace) => todo!(),
                // None => column()
                //     .push(text("No file selected"))
                //     .push(or_error(file_picker(&PathBuf::from(".")))),
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
        let app = container(content).max_width(1200).center_x();
        container(app).center_x().into()
    }
}
pub mod style {
    pub struct Title;
}
