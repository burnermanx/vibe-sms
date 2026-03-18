use std::sync::Arc;
use glow::HasContext;

const SMS_W: usize = 256;
const SMS_H: usize = 192;
const GG_W:  usize = 160;
const GG_H:  usize = 144;

const VERT_SRC: &str = r#"#version 330 core
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
out vec2 v_uv;
void main() {
    gl_Position = vec4(a_pos, 0.0, 1.0);
    v_uv = a_uv;
}
"#;

const FRAG_SRC: &str = r#"#version 330 core
uniform sampler2D u_tex;
in vec2 v_uv;
out vec4 out_color;
void main() {
    out_color = texture(u_tex, v_uv);
}
"#;

pub struct Renderer {
    program: glow::Program,
    vao:     glow::VertexArray,
    vbo:     glow::Buffer,
    texture: glow::Texture,
}

impl Renderer {
    pub fn new(gl: &Arc<glow::Context>) -> Self {
        unsafe {
            // Shader program
            let vs = gl.create_shader(glow::VERTEX_SHADER).unwrap();
            gl.shader_source(vs, VERT_SRC);
            gl.compile_shader(vs);
            assert!(gl.get_shader_compile_status(vs), "VS: {}", gl.get_shader_info_log(vs));

            let fs = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
            gl.shader_source(fs, FRAG_SRC);
            gl.compile_shader(fs);
            assert!(gl.get_shader_compile_status(fs), "FS: {}", gl.get_shader_info_log(fs));

            let program = gl.create_program().unwrap();
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            assert!(gl.get_program_link_status(program), "{}", gl.get_program_info_log(program));
            gl.detach_shader(program, vs);
            gl.detach_shader(program, fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);

            // Texture (256×192 RGBA, NEAREST)
            let texture = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            let zeros = vec![0u8; SMS_W * SMS_H * 4];
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::RGBA as i32,
                SMS_W as i32, SMS_H as i32, 0,
                glow::RGBA, glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&zeros)),
            );

            // VAO + VBO (6 vertices, will be updated each draw)
            let vao = gl.create_vertex_array().unwrap();
            let vbo = gl.create_buffer().unwrap();
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            // layout: x, y, u, v  (4 f32 per vertex)
            let stride = (4 * std::mem::size_of::<f32>()) as i32;
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 2 * std::mem::size_of::<f32>() as i32);
            gl.enable_vertex_attrib_array(1);

            Self { program, vao, vbo, texture }
        }
    }

    pub fn upload_frame(&self, gl: &Arc<glow::Context>, fb: &[u32]) {
        let rgba: Vec<u8> = fb.iter().flat_map(|&p| {
            [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255u8]
        }).collect();
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D, 0, 0, 0,
                SMS_W as i32, SMS_H as i32,
                glow::RGBA, glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&rgba)),
            );
        }
    }

    pub fn draw(&self, gl: &Arc<glow::Context>, window_size: (u32, u32), is_gg: bool, top_offset_px: u32) {
        let win_w = window_size.0 as f32;
        let win_h_total = window_size.1 as f32;
        let top = top_offset_px as f32;
        // Letterbox within the area below the menu bar
        let avail_h = win_h_total - top;
        let (emu_w, emu_h) = if is_gg {
            (GG_W as f32, GG_H as f32)
        } else {
            (SMS_W as f32, SMS_H as f32)
        };
        let aspect = emu_w / emu_h;
        let (rect_w, rect_h) = if win_w / avail_h > aspect {
            (avail_h * aspect, avail_h)
        } else {
            (win_w, win_w / aspect)
        };
        let x0 = (win_w - rect_w) * 0.5;
        let y0 = top + (avail_h - rect_h) * 0.5;
        let x1 = x0 + rect_w;
        let y1 = y0 + rect_h;

        // Convert pixel rect to NDC (y=0 is top in pixel space, y=1 is top in NDC)
        let to_ndc_x = |x: f32| x / win_w * 2.0 - 1.0;
        let to_ndc_y = |y: f32| 1.0 - y / win_h_total * 2.0;
        let nx0 = to_ndc_x(x0); let nx1 = to_ndc_x(x1);
        let ny0 = to_ndc_y(y0); let ny1 = to_ndc_y(y1);

        // UV coords — if GG, show only the 160×144 window out of 256×192 texture
        let (u0, v0, u1, v1) = if is_gg {
            let ox = 48.0f32 / SMS_W as f32;
            let oy = 24.0f32 / SMS_H as f32;
            let ux = ox + GG_W as f32 / SMS_W as f32;
            let vy = oy + GG_H as f32 / SMS_H as f32;
            (ox, oy, ux, vy)
        } else {
            (0.0f32, 0.0f32, 1.0f32, 1.0f32)
        };

        #[rustfmt::skip]
        let verts: [f32; 24] = [
            nx0, ny1, u0, v1,   // bottom-left
            nx1, ny1, u1, v1,   // bottom-right
            nx1, ny0, u1, v0,   // top-right
            nx0, ny1, u0, v1,   // bottom-left
            nx1, ny0, u1, v0,   // top-right
            nx0, ny0, u0, v0,   // top-left
        ];
        let bytes = bytemuck_cast(&verts);

        unsafe {
            gl.viewport(0, 0, window_size.0 as i32, window_size.1 as i32);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);

            gl.use_program(Some(self.program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            let loc = gl.get_uniform_location(self.program, "u_tex");
            gl.uniform_1_i32(loc.as_ref(), 0);

            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::DYNAMIC_DRAW);
            gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }
    }

    pub fn destroy(&self, gl: &Arc<glow::Context>) {
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vbo);
            gl.delete_texture(self.texture);
        }
    }
}

fn bytemuck_cast(data: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            data.as_ptr() as *const u8,
            std::mem::size_of_val(data),
        )
    }
}
