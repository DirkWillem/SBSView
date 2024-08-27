mod view;
mod views;
mod signals;

use crate::view::{ChildView, State, UpdateTopLevelView, View};
use crate::views::main_view::MainView;
use eframe::egui;
use eframe::egui::{Style, Visuals};
use sbs_core::sbs::Client;


#[tokio::main]
async fn main() {
    let mut native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| {
            let style = Style {
                visuals: Visuals::dark(),
                ..Style::default()
            };
            cc.egui_ctx.set_style(style);
            Ok(Box::new(MyEguiApp::new(cc)))
        }),
    )
        .unwrap();
}

struct MyEguiApp {
    main_view: MainView,
}

impl MyEguiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut result = MyEguiApp {
            main_view: MainView::new(),
        };

        result
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.main_view.update(ctx, frame);
        // egui::CentralPanel::default().show(ctx, |ui| {
        //     ui.with_layout(Layout::top_down(Align::Center).with_cross_align(Align::LEFT).with_main_align(Align::Center).with_main_justify(false).with_cross_justify(false), |ui| {
        //         self.methods_view.render(ui);
        //     });
        // });
    }
}
