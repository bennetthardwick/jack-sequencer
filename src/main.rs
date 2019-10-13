use crossbeam_channel::bounded;
use gdk::WindowTypeHint;
use gio::prelude::*;
use gio::File;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindowBuilder, FileChooserAction, WindowType};
use jack::{AudioIn, AudioOut, Client, ClientOptions};
use std::i16;

const NUM_BARS: usize = 4;
const BEATS_PER_BAR: usize = 4;
const NUM_TRACKS: usize = 6;
const DEFAULT_BPM: usize = 240;
const NAME: &'static str = "rust_sequencer";
const OUT_L: &'static str = "Left";
const OUT_R: &'static str = "Right";

#[derive(Clone)]
struct Sample {
    sample_rate: usize,
    data: Vec<Vec<f32>>
}

enum Message {
    UpdateSequencer((usize, usize, bool)),
    UpdateFile((usize, Sample)),
    Play,
    Pause,
    Quit,
}

#[derive(Clone)]
struct State {
    files: Vec<Option<Sample>>,
    sequencer: Vec<Vec<bool>>,
    playing: bool,
    bpm: usize,
}

impl State {
    fn new() -> State {
        State {
            files: vec![None; NUM_TRACKS],
            sequencer: vec![vec![false; BEATS_PER_BAR * NUM_BARS]; NUM_TRACKS],
            playing: false,
            bpm: DEFAULT_BPM,
        }
    }
}

struct Looper {
    beat: usize,
    sample: usize,
    samples_for_beat: usize,
}

impl Looper {
    fn new(bpm: usize, rate: usize) -> Looper {
        let samples_for_beat = (rate * 60) / bpm;
        Looper {
            beat: 0,
            sample: 0,
            samples_for_beat,
        }
    }
}

impl Iterator for Looper {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        self.sample += 1;
        let remainder = self.sample % self.samples_for_beat;
        let new_beats = self.sample / self.samples_for_beat;
        self.sample = remainder;
        self.beat += new_beats;
        self.beat = self.beat % (BEATS_PER_BAR * NUM_BARS);

        Some((self.beat, self.sample))
    }
}

fn main() {
    let application = Application::new(
        Some(format!("com.github.bennetthardwick.{}", NAME).as_str()),
        Default::default(),
    )
    .expect("failed to initialize GTK application");

    let (message_tx, message_rx) = bounded::<Message>(5);
    let (state_tx, state_rx) = bounded::<State>(5);

    let message_tx_1 = message_tx.clone();
    application.connect_activate(move |app| {
        let window = ApplicationWindowBuilder::new()
            .application(app)
            .title("Mixer")
            .type_hint(WindowTypeHint::Utility)
            .default_width(0)
            .width_request(0)
            .default_height(0)
            .height_request(0)
            .resizable(false)
            .type_(WindowType::Toplevel)
            .build();

        let header = gtk::HeaderBarBuilder::new().title("Sequencer").build();

        let container = gtk::Box::new(gtk::Orientation::Vertical, 25);
        container.pack_start(&header, false, false, 0);

        for track in 0..NUM_TRACKS {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);

            let filter = gtk::FileFilter::new();
            filter.add_pattern("*.wav");

            let chooser = gtk::FileChooserButtonBuilder::new()
                .title("Load File")
                .margin_start(25)
                .build();
            chooser.set_filter(&filter);
            {
                let message_tx = message_tx.clone();
                chooser.connect_file_set(move |f| {
                    if let Some(file) = f.get_filename() {
                        if let Ok(reader) = hound::WavReader::open(file) {
                            println!("Loading samples!");
                            let spec = reader.spec();
                            let sample_rate = spec.sample_rate as usize;
                            let samples = reader.into_samples();
                            let data = samples
                                .map(|s| s.unwrap())
                                .collect::<Vec<i16>>()
                                .chunks(spec.channels as usize)
                                .map(|x| {
                                    x.iter().map(|y| (*y as f32) / (i16::MAX as f32)).collect()
                                })
                                .collect::<Vec<Vec<f32>>>();
                            println!("File has {} samples... sending message!", data.len());
                            message_tx
                                .send(Message::UpdateFile((track, Sample { data, sample_rate })))
                                .unwrap();
                        }
                    }
                });
            }
            row.pack_start(&chooser, true, true, 5);

            for bar in 0..NUM_BARS {
                for beat in 0..BEATS_PER_BAR {
                    let mut button = gtk::ToggleButtonBuilder::new()
                        .opacity(if beat == 0 { 1. } else { 0.4 });

                    if bar == NUM_BARS - 1 && beat == BEATS_PER_BAR - 1 {
                        button = button.margin_end(25);
                    }

                    let button = button.build();
                    {
                        let message_tx = message_tx.clone();
                        button.connect_toggled(move |b| {
                            message_tx
                                .send(Message::UpdateSequencer((
                                    track,
                                    (bar * BEATS_PER_BAR) + beat,
                                    b.get_active(),
                                )))
                                .unwrap();
                        });
                    }

                    row.pack_start(&button, false, false, 5);
                }
            }

            container.pack_start(&row, false, false, 5);
            container.set_margin_bottom(25);
        }
        window.add(&container);
        window.show_all();
    });

    let client = Client::new(NAME, ClientOptions::NO_START_SERVER)
        .unwrap()
        .0;

    let rate = client.sample_rate();

    let out_spec = AudioOut::default();

    let mut out_l_port = client.register_port(OUT_L, out_spec).unwrap();
    let mut out_r_port = client.register_port(OUT_R, out_spec).unwrap();
    let reducer_thread = std::thread::spawn(move || {
        let mut state = State::new();

        for message in message_rx.iter() {
            match message {
                Message::UpdateFile((index, samples)) => {
                    state.files[index] = Some(samples);
                }
                Message::UpdateSequencer((track, beat, active)) => {
                    state.sequencer[track][beat] = active;
                }
                Message::Quit => {
                    return;
                },
                _ => {
                    unimplemented!();
                }
            }

            if let Err(e) = state_tx.try_send(state.clone()) {
                println!("Error: {:?}", e);
            }
        }
    });

    // The state that will live inside the audio thread
    let mut state = State::new();
    let mut looper = Looper::new(DEFAULT_BPM, rate);

    let process = jack::ClosureProcessHandler::new(
        move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
            for new_state in state_rx.try_iter() {
                state = new_state;
            }

            let out_l = out_l_port.as_mut_slice(ps);
            let out_r = out_r_port.as_mut_slice(ps);

            for ((l, r), (beat, sample)) in out_l.iter_mut().zip(out_r.iter_mut()).zip(&mut looper)
            {

                *l = 0.;
                *r = 0.;

                for (i, track) in state.files.iter().enumerate().filter_map(|(i, track)| {
                    if let Some(track) = track {
                        Some((i, track))
                    } else {
                        None
                    }
                }) {
                    if state.sequencer[i][beat] {
                        if let Some(t) = track.data.get((sample as f32 * (track.sample_rate as f32 / rate as f32)) as usize) {
                            if t.len() == 1 {
                                *l += t[0];
                                *r += t[0];
                            } else if t.len() >= 2 {
                                *l += t[0];
                                *r += t[1];
                            }
                        }
                    }
                }
            }

            jack::Control::Continue
        },
    );

    let active = client.activate_async((), process).unwrap();

    let system_out_1 = active.as_client().port_by_name("system:playback_1").unwrap();
    let system_out_2 = active.as_client().port_by_name("system:playback_2").unwrap();
    let out_l_port = active.as_client().port_by_name(format!("{}:{}", NAME, OUT_L).as_str()).unwrap();
    let out_r_port = active.as_client().port_by_name(format!("{}:{}", NAME, OUT_R).as_str()).unwrap();
    active.as_client().connect_ports(&out_r_port, &system_out_1).unwrap();
    active.as_client().connect_ports(&out_l_port, &system_out_2).unwrap();

    application.run(&[]);
    active.deactivate().unwrap();
    message_tx_1.send(Message::Quit).unwrap();
    reducer_thread.join().unwrap();
}
