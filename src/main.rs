use board::{
    Arrival, fetch_with_retry, format_clock, format_now, format_time, recalculate_and_filter,
};
use slint::{Model, VecModel};
#[cfg(feature = "framebuffer")]
use slint_backend_linuxfb::LinuxFbPlatformBuilder;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

slint::slint! {
export struct ArrivalData {
    direction: string,
    train: string,
    service: string,
    destination: string,
    departure: string,
    minutes: string,
    route_color: color,
}

export component MainUI inherits Window {
    in-out property <[ArrivalData]> arrivals;
    in-out property <string> current_time;
    in-out property <string> stop_name;
    in-out property <string> status;
    title: "Redwood City Caltrain";
    preferred-width: 800px;
    preferred-height: 480px;
    background: black;

    VerticalLayout {
        alignment: start;
        padding-left: 32px;
        padding-right: 32px;
        padding-top: 24px;
        padding-bottom: 24px;
        spacing: 4px;

        Rectangle {
            height: 40px;

            Text {
                x: 0;
                text: current_time;
                font-size: 16pt;
                font-weight: 700;
                color: #c8c8c8;
                vertical-alignment: center;
            }

            Text {
                text: stop_name;
                font-size: 22pt;
                font-weight: 800;
                color: white;
                horizontal-alignment: center;
                vertical-alignment: center;
                width: 100%;
            }
        }

        Rectangle {
            height: 4px;
            background: #e31837;
        }

        HorizontalLayout {
            spacing: 8px;

            Rectangle { width: 42px; height: 1px; }

            Text {
                text: "Train";
                width: 70px;
                font-size: 12pt;
                font-weight: 700;
                color: #8d8d8d;
                vertical-alignment: center;
            }

            Text {
                text: "Type";
                width: 84px;
                font-size: 12pt;
                font-weight: 700;
                color: #8d8d8d;
                vertical-alignment: center;
            }

            Text {
                text: "Destination";
                horizontal-stretch: 1;
                font-size: 12pt;
                font-weight: 700;
                color: #8d8d8d;
                vertical-alignment: center;
            }

            Text {
                text: "Departs";
                width: 112px;
                font-size: 12pt;
                font-weight: 700;
                color: #8d8d8d;
                horizontal-alignment: right;
                vertical-alignment: center;
            }

            Text {
                text: "In";
                width: 56px;
                font-size: 12pt;
                font-weight: 700;
                color: #8d8d8d;
                horizontal-alignment: right;
                vertical-alignment: center;
            }
        }

        Text {
            text: status;
            font-size: 16pt;
            color: #d0d0d0;
            wrap: word-wrap;
            min-height: status == "" ? 0px : 20px;
            max-height: status == "" ? 0px : 48px;
        }

        for arrival in arrivals: HorizontalLayout {
            spacing: 8px;

            Rectangle {
                width: 42px;
                height: 42px;
                border-radius: 21px;
                background: arrival.route_color;

                Text {
                    x: 0px;
                    y: 3px;
                    width: parent.width;
                    height: parent.height;
                    text: arrival.direction;
                    font-size: 18pt;
                    font-weight: 800;
                    color: white;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }
            }

            Text {
                text: arrival.train;
                width: 70px;
                font-size: 20pt;
                font-weight: 800;
                color: white;
                vertical-alignment: center;
            }

            Text {
                text: arrival.service;
                width: 84px;
                font-size: 14pt;
                font-weight: 700;
                color: #e6e6e6;
                vertical-alignment: center;
            }

            Text {
                text: arrival.destination;
                font-size: 20pt;
                font-weight: 800;
                color: white;
                vertical-alignment: center;
                horizontal-stretch: 1;
                overflow: elide;
                wrap: no-wrap;
            }

            Text {
                text: arrival.departure;
                width: 112px;
                font-size: 16pt;
                font-weight: 800;
                color: white;
                horizontal-alignment: right;
                vertical-alignment: center;
            }

            Text {
                text: arrival.minutes;
                width: 56px;
                font-size: 16pt;
                font-weight: 800;
                color: white;
                horizontal-alignment: right;
                vertical-alignment: center;
            }
        }
    }
}
}

struct State {
    main_ui: MainUI,
    raw_arrivals: RefCell<Vec<Arrival>>,
    clock_offset_ms: Cell<i64>,
    loaded: Cell<bool>,
    arrivals_model: Rc<VecModel<ArrivalData>>,
}

async fn fetch_arrivals(state: &State) {
    match fetch_with_retry().await {
        Ok((arrivals, clock_offset)) => {
            state.clock_offset_ms.set(clock_offset);
            *state.raw_arrivals.borrow_mut() = arrivals;
            state.loaded.set(true);
            refresh_display(state);
        }
        Err(err) => {
            eprintln!("caltrain fetch failed: {err}");
            if !state.loaded.get() {
                state
                    .main_ui
                    .set_status("Couldn't reach Caltrain. Retrying…".into());
            }
        }
    }
}

fn time_offset_ms(state: &State) -> i64 {
    // A synced system clock is already the real time. Adding the feed's
    // timestamp on top of it runs the board slow by however stale that
    // header is (about two minutes on Caltrain's feed).
    if board::clock::CLOCK_SYNCED.load(std::sync::atomic::Ordering::Relaxed) {
        0
    } else {
        state.clock_offset_ms.get()
    }
}

fn refresh_display(state: &State) {
    if !state.loaded.get() {
        return;
    }

    let mut arrivals = state.raw_arrivals.borrow().clone();
    recalculate_and_filter(&mut arrivals, time_offset_ms(state));

    if arrivals.is_empty() {
        state.main_ui.set_status("No upcoming trains".into());
    } else {
        state.main_ui.set_status("".into());
    }

    let model = &state.arrivals_model;
    for (i, arrival) in arrivals.iter().enumerate() {
        let [r, g, b] = arrival.route_color;
        let data = ArrivalData {
            direction: arrival.route_label.clone().into(),
            train: arrival.trip_id.clone().into(),
            service: arrival.service.clone().into(),
            destination: arrival.destination.clone().into(),
            departure: format_clock(arrival.departure_time_ms).into(),
            minutes: format_time(arrival.minutes).into(),
            route_color: slint::Color::from_rgb_u8(r, g, b),
        };
        if i < model.row_count() {
            model.set_row_data(i, data);
        } else {
            model.push(data);
        }
    }
    while model.row_count() > arrivals.len() {
        model.remove(model.row_count() - 1);
    }
}

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _tokio = rt.enter();

    #[cfg(feature = "framebuffer")]
    {
        let platform = LinuxFbPlatformBuilder::new()
            .with_framebuffer("/dev/fb0")
            .with_input_autodiscovery(true)
            .build()
            .unwrap();
        slint::platform::set_platform(Box::new(platform)).unwrap();
    }

    let arrivals_model = Rc::new(VecModel::<ArrivalData>::default());
    let main_ui = MainUI::new().unwrap();
    main_ui.set_arrivals(arrivals_model.clone().into());
    main_ui.set_stop_name("Redwood City".into());
    main_ui.set_status("Loading trains…".into());

    let state = Rc::new(State {
        main_ui,
        raw_arrivals: RefCell::new(Vec::new()),
        clock_offset_ms: Cell::new(0),
        loaded: Cell::new(false),
        arrivals_model,
    });

    {
        let state = state.clone();
        slint::spawn_local(async move {
            let mut tick = 0u32;
            let mut was_synced = false;
            loop {
                let synced = time_offset_ms(&state) == 0
                    && board::clock::CLOCK_SYNCED.load(std::sync::atomic::Ordering::Relaxed);
                let now = chrono::Local::now()
                    + chrono::Duration::milliseconds(time_offset_ms(&state));
                state
                    .main_ui
                    .set_current_time(format_now(&now).into());
                if tick % 10 == 0 || (synced && !was_synced) {
                    refresh_display(&state);
                }
                was_synced = synced;
                tick = tick.wrapping_add(1);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
        .unwrap();
    }

    slint::spawn_local(board::clock::run_clock_sync()).unwrap();

    {
        let state = state.clone();
        slint::spawn_local(async move {
            loop {
                fetch_arrivals(&state).await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        })
        .unwrap();
    }

    state.main_ui.run().unwrap();
}
