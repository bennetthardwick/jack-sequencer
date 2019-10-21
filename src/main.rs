use crossbeam_channel::bounded;
use gdk::WindowTypeHint;
use gio::prelude::*;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindowBuilder, WindowType};
use jack::{AudioOut, Client, ClientOptions};
use std::i16;
use std::sync::Arc;

const NUM_BARS: usize = 4;
const BEATS_PER_BAR: usize = 4;
const NUM_TRACKS: usize = 20;
const DEFAULT_BPM: usize = 240;
const NAME: &str = "rust_sequencer";
const OUT_L: &str = "Left";
const OUT_R: &str = "Right";

#[derive(Clone)]
struct AudioTrack {
    sample: Arc<Sample>,
    playing: bool,
    offset: usize,
}

impl AudioTrack {
    fn new(sample: Arc<Sample>) -> AudioTrack {
        AudioTrack { sample, playing: false, offset: 0 }
    }

    fn reset(&mut self) {
        self.offset = 0;
        self.playing = true;
    }

    fn progress(&mut self) -> (f32, f32) {
        let output;

        if self.playing && self.offset * self.sample.channels + self.sample.channels <= self.sample.data.len() {
            if self.sample.channels == 1 {
                output = (self.sample.data[self.offset], self.sample.data[self.offset])
            } else if self.sample.channels >= 2 {
                output = (
                    self.sample.data[self.offset],
                    self.sample.data[self.offset + 1],
                );
            } else {
                output = (0., 0.);
                self.playing = false;
            }
        } else {
            self.playing = false;
            output = (0., 0.);
        }
        self.offset += 1;
        return output;
    }
}

#[derive(Clone)]
struct State {
    tracks: Vec<Option<AudioTrack>>,
    playing: bool,
    bpm: usize,
    sequencer: [[bool; NUM_BARS * BEATS_PER_BAR]; NUM_TRACKS],
}

#[derive(Clone)]
struct Sample {
    sample_rate: usize,
    channels: usize,
    data: Vec<f32>,
}

// Messages are information that gets sent
enum Message {
    UpdateSequencer((usize, usize, bool)),
    UpdateFile((usize, Sample)),
    Play,
    Pause,
    Quit,
}

impl State {
    fn new() -> State {
        State {
            tracks: vec![None; NUM_TRACKS],
            sequencer: [[false; NUM_BARS * BEATS_PER_BAR]; NUM_TRACKS],
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
        // Increment the current sample count / offset.
        // If the number of samples is greater than the number required
        // for a beat, increment beat. If the beats are greater than
        // the number of beats for the entire composition, set it back to zero.
        // Note: this is built to work by incrementing the sample offset
        // by an arbitrary amount, but it's nice to use it as an iterator
        // so I kept it at one.
        self.sample += 1;
        let remainder = self.sample % self.samples_for_beat;
        let new_beats = self.sample / self.samples_for_beat;
        self.sample = remainder;
        self.beat += new_beats;
        self.beat %= BEATS_PER_BAR * NUM_BARS;

        Some((self.beat, self.sample))
    }
}

fn main() {
    let application = Application::new(
        Some(format!("com.github.bennetthardwick.{}", NAME).as_str()),
        Default::default(),
    )
    .expect("failed to initialize GTK application");

    // Create channels for messages and for sending the updated
    // state. Updating the state on a different thread and sending it
    // into the audio thread is a good way to ensure no time is spent
    // allocating large arrays on that real-time thread.
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

        let play_pause = gtk::ToggleButtonBuilder::new()
            .active(true)
            .margin_start(25)
            .margin_end(25)
            .label("Playing")
            .build();

        {
            let message_tx = message_tx.clone();
            play_pause.connect_toggled(move |b| {
                if b.get_active() {
                    message_tx.send(Message::Play).unwrap();
                } else {
                    message_tx.send(Message::Pause).unwrap();
                }
            });
        }

        container.pack_start(&play_pause, false, false, 0);

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
                            let channels = spec.channels as usize;
                            let data = samples
                                .map(|s| s.unwrap())
                                .collect::<Vec<i16>>()
                                .iter()
                                .map(|y| (*y as f32) / (i16::MAX as f32))
                                .collect::<Vec<f32>>();
                            println!("File has {} samples... sending message!", data.len());
                            message_tx
                                .send(Message::UpdateFile((
                                    track,
                                    Sample {
                                        data,
                                        channels,
                                        sample_rate,
                                    },
                                )))
                                .unwrap();
                        }
                    }
                });
            }
            row.pack_start(&chooser, true, true, 5);

            for bar in 0..NUM_BARS {
                for beat in 0..BEATS_PER_BAR {
                    let mut button =
                        gtk::ToggleButtonBuilder::new().opacity(if beat == 0 { 1. } else { 0.4 });

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

    // Initialise the jack client.
    let client = Client::new(NAME, ClientOptions::NO_START_SERVER).unwrap().0;
    let rate = client.sample_rate();
    let out_spec = AudioOut::default();
    let mut out_l_port = client.register_port(OUT_L, out_spec).unwrap();
    let mut out_r_port = client.register_port(OUT_R, out_spec).unwrap();

    // Inspired by redux reducers ;)
    let reducer_thread = std::thread::spawn(move || {
        let mut state = State::new();

        for message in message_rx.iter() {
            match message {
                Message::UpdateFile((index, sample)) => {
                    if let Some(track) = state.tracks[index].as_mut() {
                        (*track).sample = Arc::new(sample);
                    } else {
                        state.tracks[index] = Some(AudioTrack::new(Arc::new(sample)));
                    }
                }
                Message::UpdateSequencer((track, beat, active)) => {
                    state.sequencer[track][beat] = active;
                }
                Message::Play => state.playing = true,
                Message::Pause => state.playing = false,
                Message::Quit => {
                    return;
                }
                _ => {
                    unimplemented!();
                }
            }

            // This will only error when the buffer is full, which should never
            // really happen.
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
            // state_rx can receive data in two ways, .iter() and .try_iter().
            // The first takes a lock and blocks execution until a message is
            // received. The second doesn't and consumes only data that is in the buffer
            // at the time of checking. On the audio thread, if execution is blocked it
            // can cause loud crackles and pops - which isn't great.
            for new_state in state_rx.try_iter() {
                state = new_state;
            }

            let out_l = out_l_port.as_mut_slice(ps);
            let out_r = out_r_port.as_mut_slice(ps);

            // Progress throughout the song is maintained using the "looper"
            // iterator. If it's never called then the song is effectively paused.
            if !state.playing {
                // the out_l and out_r buffers are reused on every "process".
                // Thus they have to be set to 0 just in case samples are left over
                // from the last run.
                for (l, r) in out_l.iter_mut().zip(out_r.iter_mut()) {
                    *l = 0.;
                    *r = 0.;
                }

                // Return early and tell jack to continue
                return jack::Control::Continue;
            }

            // Progress the song by the same number of samples that the buffer contains
            for ((l, r), (beat, sample)) in out_l.iter_mut().zip(out_r.iter_mut()).zip(&mut looper)
            {
                // Loop through all the tracks and filter by those that have a sample loaded in.
                // Note: it feels a little bit weird to be doing this for sample in the output
                // buffer - but it seemed like the best way (at the time) to avoid allocating an
                // intermediate Vec on the audio thread.

                let tracks = &mut (state.tracks);
                let sequencer = &state.sequencer;

                let (t_l, t_r) = tracks
                    .iter_mut()
                    .enumerate()
                    .filter_map(|(i, track)| {
                        if let Some(track) = track {
                            // If at the very start of a beat for this track,
                            // reset the sample.
                            if sequencer[i][beat] && sample == 0 {
                                track.reset();
                            }
                            
                            // Progress the sample and return the l and r samples
                            return Some(track.progress());
                        }

                        None
                    })
                    // Add together all the samples from each track
                    .fold((0., 0.), |(a1, a2), (b1, b2)| (a1 + b1, a2 + b2));

                *l = t_l;
                *r = t_r;
            }

            jack::Control::Continue
        },
    );

    // Activate the jack client and start processing audio
    let active = client.activate_async((), process).unwrap();

    // Automatically connect the application's outputs to the
    // system outputs. If this wasn't done you'd need to use software
    // like "Catia" to manually wire it up after starting.
    // Note: this needs to be done after the client has been activated,
    // as per: https://github.com/RustAudio/rust-jack/issues/100
    let system_out_1 = active
        .as_client()
        .port_by_name("system:playback_1")
        .unwrap();
    let system_out_2 = active
        .as_client()
        .port_by_name("system:playback_2")
        .unwrap();
    let out_l_port = active
        .as_client()
        .port_by_name(format!("{}:{}", NAME, OUT_L).as_str())
        .unwrap();
    let out_r_port = active
        .as_client()
        .port_by_name(format!("{}:{}", NAME, OUT_R).as_str())
        .unwrap();
    active
        .as_client()
        .connect_ports(&out_l_port, &system_out_1)
        .unwrap();
    active
        .as_client()
        .connect_ports(&out_r_port, &system_out_2)
        .unwrap();

    // Start the GTK application. This locks the thread until
    // some kind of close signal is sent to the application.
    application.run(&[]);
    // After the GTK has closed, close the jack server.
    active.deactivate().unwrap();
    // Send the quit signal to the thread that updates the state,
    // so everything can gracefully exit.
    message_tx_1.send(Message::Quit).unwrap();
    // Wait for that thread to close because exiting the program.
    reducer_thread.join().unwrap();
}
