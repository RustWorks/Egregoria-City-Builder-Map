use super::{InspectArgsDefault, InspectRenderDefault};

impl InspectRenderDefault<i32> for i32 {
    fn render(data: &Self, label: &'static str, ui: &mut egui::Ui, _args: &InspectArgsDefault) {
        // Values are consistent
        let mut cp = *data;
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut cp));
            ui.label(label);
        });
    }

    fn render_mut(
        data: &mut Self,
        label: &'static str,
        ui: &mut egui::Ui,
        args: &InspectArgsDefault,
    ) -> bool {
        let before = *data;
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(egui::DragValue::new(data).clamp_range(
                args.min_value.unwrap_or(f32::MIN)..=args.max_value.unwrap_or(f32::MAX),
            ));
        });
        before != *data
    }
}