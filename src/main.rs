use crossbeam_channel::bounded;
use gdk::WindowTypeHint;
use gio::prelude::*;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindowBuilder, WindowType};
use jack::{AudioIn, AudioOut, Client, ClientOptions};

macro_rules! section {
    ($container:expr) => {{
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);

        let combo = gtk::ComboBoxBuilder::new()
            .margin_start(25)
            .build();
        row.pack_start(&combo, true, true, 5);

        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(1.)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);

        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(1.)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);

        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(1.)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);

        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(1.)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .build();
        row.pack_start(&button, false, false, 5);
        let button = gtk::ToggleButtonBuilder::new()
            .label(" ")
            .opacity(0.5)
            .margin_start(25)
            .build();
        row.pack_start(&button, false, false, 5);

        $container.pack_start(&row, false, false, 5);
    }};
}

fn main() {
    let application = Application::new(
        Some("com.github.bennetthardwick.rust-mixer"),
        Default::default(),
    )
    .expect("failed to initialize GTK application");

    application.connect_activate(move |app| {
        let window = ApplicationWindowBuilder::new()
            .application(app)
            .title("Mixer")
            .type_hint(WindowTypeHint::Utility)
            .default_width(1100)
            .width_request(1100)
            .default_height(450)
            .height_request(450)
            .resizable(false)
            .type_(WindowType::Toplevel)
            .build();

        let header = gtk::HeaderBarBuilder::new().title("Sequencer").build();

        let container = gtk::Box::new(gtk::Orientation::Vertical, 25);
        container.pack_start(&header, false, false, 0);

        let chooser = gtk::FileChooserButtonBuilder::new()
            .title("Load File")
            .margin_start(25)
            .margin_end(25)
            .build();
        container.pack_start(&chooser, false, false, 0);

        section!(container);
        section!(container);
        section!(container);
        section!(container);
        section!(container);
        section!(container);
        
        window.add(&container);
        window.show_all();
    });

    let client = Client::new("rust_mixer", ClientOptions::NO_START_SERVER)
        .unwrap()
        .0;

    let out_spec = AudioOut::default();

    let mut out_l_port = client.register_port("Left", out_spec).unwrap();
    let mut out_r_port = client.register_port("Right", out_spec).unwrap();

    application.run(&[]);
}
