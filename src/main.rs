use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::{Duration, SystemTime};
use druid::{
    widget::{TextBox, Flex, Button, Scroll, Container},
    AppLauncher, Color, Env, Event, EventCtx, Insets, PlatformError,
    Selector, Widget, WidgetExt, WindowDesc, Data, Lens, Target,
};
use tokio_stream::{wrappers::ReadDirStream, StreamExt};
use tokio::fs;
use chrono::{DateTime, Local};
use tokio::io;

// Define selectors for commands
const PURGE_COMMAND: Selector<()> = Selector::new("purge");
const STOP_COMMAND: Selector<()> = Selector::new("stop");
const UPDATE_OUTPUT: Selector<String> = Selector::new("update_output"); // New selector for UI update

// Define a struct to hold file information
#[derive(Clone, Data, Debug)]
struct FileEntry {
    path: String,
    modified: String,
}

#[derive(Clone, Data, Lens)]
struct AppState {
    output: String,
    is_purging: bool,
    tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<String>>>>,
    file_list: Arc<Mutex<Vec<FileEntry>>>,
}

impl AppState {
    fn new() -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(32);
        let tx = Arc::new(Mutex::new(Some(tx)));

        // Spawn a task to handle incoming messages
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                println!("Received message: {}", message);
            }
        });

        Self {
            output: String::new(),
            is_purging: false,
            tx,
            file_list: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

// Task to simulate purging process
async fn purging_task(tx: tokio::sync::mpsc::Sender<String>) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        tx.send("Scanning...\n".into()).await.unwrap();
    }
}

// Controller for managing the UI state and tasks
struct PurgeController;

impl<W: Widget<AppState>> druid::widget::Controller<AppState, W> for PurgeController {
    fn event(&mut self, child: &mut W, ctx: &mut EventCtx, event: &Event, data: &mut AppState, env: &Env) {
        if let Event::Command(cmd) = event {
            if cmd.is(PURGE_COMMAND) {
                data.is_purging = true;
                let file_list = data.file_list.clone(); // Clone the Arc
                let directory = "C:\\Users\\orkar\\Downloads\\serverdocs"; // Set your directory path here

                // Spawn task to handle purging
                let tx = data.tx.clone();
                let ctx_for_file_listing = ctx.get_external_handle();
                tokio::spawn(async move {
                    if let Some(tx) = tx.lock().await.take() {
                        println!("Calling Purge");
                        purging_task(tx).await;
                    }
                });

                // Spawn a task to list files and update the UI
                tokio::spawn(async move {
                    let mut file_list = file_list.lock().await; // Lock the mutex
                    if let Err(e) = list_files_with_dates(directory, &mut *file_list).await {
                        eprintln!("Error: {}", e);
                    }

                    // Submit the updated file list to the UI
                    if let Err(e) = ctx_for_file_listing.submit_command(
                        UPDATE_OUTPUT,
                        format!("{:?}", *file_list),  // Payload
                        Target::Auto   // Target
                    ) {
                        eprintln!("Failed to submit command: {:?}", e);
                    }
                });
            } else if cmd.is(STOP_COMMAND) {
                data.is_purging = false;
            }
        }

        if let Event::Command(cmd) = event {
            if let Some(output) = cmd.get(UPDATE_OUTPUT) {
                data.output.push_str(output);
                ctx.request_paint(); // Force a UI repaint
            }
        }

        child.event(ctx, event, data, env);
    }
}

// Helper function to format `SystemTime` to `DateTime<Local>`
fn to_local_time(system_time: SystemTime) -> DateTime<Local> {
    DateTime::<Local>::from(system_time)
}

// Task to list files with modification times and update the file list
async fn list_files_with_dates(directory: &str, file_list: &mut Vec<FileEntry>) -> io::Result<()> {
    let read_dir = fs::read_dir(directory).await?;
    let mut entries = ReadDirStream::new(read_dir);

    while let Some(entry) = entries.next().await {
        match entry {
            Ok(entry) => {
                let metadata = entry.metadata().await?;
                if metadata.is_file() {
                    let modified_time = metadata.modified().unwrap_or(SystemTime::now());
                    let modified_date = to_local_time(modified_time);
                    file_list.push(FileEntry {
                        path: entry.path().display().to_string(),
                        modified: modified_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                    });
                }
            }
            Err(e) => {
                eprintln!("Error reading entry: {}", e);
            }
        }
    }

    Ok(())
}

// Function to build the UI components
fn build_ui() -> impl Widget<AppState> {
    let output = TextBox::new()
        .with_placeholder("Output...")
        .lens(AppState::output)
        .expand()
        .background(Color::WHITE)
        .padding(Insets::uniform(10.0));

    let scrollable_output = Scroll::new(output)
        .vertical()
        .expand_height()
        .background(Color::WHITE);

    let purge_button = Button::new("Purge")
        .on_click(|ctx, data: &mut AppState, _env| {
            data.is_purging = true;
            ctx.submit_command(PURGE_COMMAND);
        });

    let stop_button = Button::new("Stop")
        .on_click(|ctx, data: &mut AppState, _env| {
            data.is_purging = false;
            ctx.submit_command(STOP_COMMAND);
        });

    Flex::column()
        .with_child(
            Container::new(purge_button)
                .background(Color::rgb(0.2, 0.2, 0.2))
                .padding(Insets::uniform(5.0))
        )
        .with_child(
            Container::new(stop_button)
                .background(Color::rgb(0.0, 0.0, 0.0))
                .padding(Insets::uniform(5.0))
        )
        .with_flex_child(
            Container::new(scrollable_output)
                .background(Color::rgb(0.2, 0.4, 0.8))
                .padding(Insets::uniform(10.0)),
            1.0
        )
        .background(Color::rgb(0.9, 0.9, 0.9))
        .padding(Insets::uniform(10.0))
}

// Main entry point for the app
#[tokio::main]
async fn main() -> Result<(), PlatformError> {
    let initial_state = AppState::new();

    let main_window = WindowDesc::new(build_ui())
        .title("Purger App")
        .window_size((600.0, 400.0))
        .resizable(true);

    AppLauncher::with_window(main_window)
        .launch(initial_state)
}
