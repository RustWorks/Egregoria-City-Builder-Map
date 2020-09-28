use crate::context::Context;
use crate::debug::add_debug_menu;
use crate::rendering::imgui_wrapper::ImguiWrapper;
use crate::rendering::{CameraHandler, InstancedRender, MeshRenderer, RoadRenderer};
use egregoria::engine_interaction::{KeyboardInfo, MouseInfo, RenderStats, TimeInfo};
use egregoria::rendering::immediate::{ImmediateDraw, ImmediateOrder};
use egregoria::{load_from_disk, Egregoria};
use geom::Camera;
use geom::Vec2;
use gui::{FollowEntity, Gui};
use map_model::Map;
use souls::Souls;
use std::time::Instant;
use wgpu_engine::{FrameContext, GfxContext, GuiRenderContext};
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct State {
    camera: CameraHandler,
    imgui_render: ImguiWrapper,
    state: Egregoria,
    last_time: Instant,
    instanced_renderer: InstancedRender,
    road_renderer: RoadRenderer,
    gui: Gui,
    souls: Souls,
}

impl State {
    pub fn new(ctx: &mut Context) -> Self {
        let camera = common::saveload::load("camera")
            .map(|camera| CameraHandler {
                camera,
                last_pos: Vec2::ZERO,
            })
            .unwrap_or_else(|| {
                CameraHandler::new(ctx.gfx.size.0 as f32, ctx.gfx.size.1 as f32, 0.05)
            });

        let wrapper = ImguiWrapper::new(&mut ctx.gfx, &ctx.window);

        crate::rendering::prepare_background(&mut ctx.gfx);

        let mut state = egregoria::Egregoria::init();

        load_from_disk(&mut state);
        gui::setup_gui(&mut state);

        let mut gui = Gui::default();
        add_debug_menu(&mut gui);
        gui.windows
            .insert(imgui::im_str!("Debug souls"), souls::debug_souls, false);

        state.insert(camera.camera.clone());

        Self {
            camera,
            imgui_render: wrapper,
            state,
            last_time: Instant::now(),
            instanced_renderer: InstancedRender::new(&mut ctx.gfx),
            road_renderer: RoadRenderer::new(&mut ctx.gfx),
            gui,
            souls: Souls::default(),
        }
    }

    pub fn update(&mut self, ctx: &mut Context) {
        let delta = self.last_time.elapsed().as_secs_f64();
        self.last_time = Instant::now();

        self.manage_time(delta, &mut ctx.gfx);

        self.manage_io(ctx);

        self.camera.easy_camera_movement(
            ctx,
            delta as f32,
            !self.imgui_render.last_mouse_captured,
            !self.imgui_render.last_kb_captured,
        );
        *self.state.write::<Camera>() = self.camera.camera.clone();

        if !self.imgui_render.last_mouse_captured {
            self.state.write::<MouseInfo>().unprojected = self.unproject(ctx.input.mouse.screen);
        }

        self.state.run();

        self.souls.add_souls_to_empty_buildings(&mut self.state);
        self.souls.update(&mut self.state);

        self.manage_entity_follow();
        self.camera.update(ctx);
    }

    pub fn render(&mut self, ctx: &mut FrameContext) {
        let start = Instant::now();

        crate::rendering::draw_background(ctx);

        let mut tess = self.camera.culled_tesselator();

        let time: TimeInfo = *self.state.read::<TimeInfo>();
        self.road_renderer.render(
            &mut self.state.write::<Map>(),
            time.time_seconds,
            &mut tess,
            ctx,
        );

        self.instanced_renderer.render(&mut self.state, ctx);

        MeshRenderer::render(&mut self.state, &mut tess);

        {
            let objs = crate::debug::DEBUG_OBJS.lock().unwrap();
            for (val, _, obj) in &*objs {
                if *val {
                    obj(&mut tess, &mut self.state);
                }
            }
        }

        {
            let immediate = &mut *self.state.write::<ImmediateDraw>();
            for (order, col) in immediate
                .persistent_orders
                .iter()
                .copied()
                .chain(immediate.orders.drain(..))
            {
                tess.color = col.into();
                match order {
                    ImmediateOrder::Circle { pos, size } => {
                        tess.draw_circle(pos, 3.0, size);
                    }
                    ImmediateOrder::Line { from, to } => {
                        tess.draw_line(from, to, 3.0);
                    }
                }
            }
        }

        if let Some(x) = tess.meshbuilder.build(ctx.gfx) {
            ctx.draw(x)
        }

        self.state
            .write::<RenderStats>()
            .render
            .add_value(start.elapsed().as_secs_f32());
    }

    pub fn render_gui(&mut self, window: &Window, ctx: GuiRenderContext) {
        self.imgui_render
            .render(ctx, window, &mut self.state, &mut self.gui);
    }

    fn manage_time(&mut self, delta: f64, gfx: &mut GfxContext) {
        const MAX_TIMESTEP: f64 = 1.0 / 10.0;
        let mut time = self.state.write::<TimeInfo>();

        let delta = (delta * time.time_speed as f64).min(MAX_TIMESTEP);
        time.delta = delta as f32;
        time.time += time.delta as f64;
        time.time_seconds = time.time as u64;

        gfx.set_time(time.time as f32);
    }

    fn manage_entity_follow(&mut self) {
        if !self.state.read::<MouseInfo>().just_pressed.is_empty() {
            self.state.write::<FollowEntity>().0.take();
        }

        if let Some(e) = self.state.read::<FollowEntity>().0 {
            if let Some(pos) = self.state.pos(e) {
                self.camera.camera.position = [pos.x, pos.y].into();
            }
        }
    }

    fn manage_io(&mut self, ctx: &Context) {
        *self.state.write::<KeyboardInfo>() = ctx.input.keyboard.clone();
        *self.state.write::<MouseInfo>() = ctx.input.mouse.clone();

        if self.imgui_render.last_kb_captured {
            let kb: &mut KeyboardInfo = &mut self.state.write::<KeyboardInfo>();
            kb.just_pressed.clear();
            kb.is_pressed.clear();
        }

        if self.imgui_render.last_mouse_captured {
            let mouse: &mut MouseInfo = &mut self.state.write::<MouseInfo>();
            mouse.just_pressed.clear();
            mouse.buttons.clear();
            mouse.wheel_delta = 0.0;
        }
    }

    pub fn event(&mut self, window: &Window, event: &winit::event::Event<()>) {
        self.imgui_render.handle_event(window, event);
    }

    pub fn resized(&mut self, ctx: &mut Context, size: PhysicalSize<u32>) {
        self.camera
            .resize(ctx, size.width as f32, size.height as f32);
    }

    pub fn unproject(&self, pos: Vec2) -> Vec2 {
        self.camera.unproject_mouse_click(pos)
    }
}