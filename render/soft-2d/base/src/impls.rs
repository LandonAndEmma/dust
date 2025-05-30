use crate::{RenderingData, Vram as VramTrait};
use dust_core::{
    gpu::{
        engine_2d::{
            AffineBgIndex, BgControl, BgIndex, BrightnessControl, ColorEffectsControl, Control,
            Engine2d, Role,
        },
        vram::Vram,
    },
    utils::Bytes,
};

impl<R: Role> RenderingData for Engine2d<R> {
    fn control(&self) -> Control {
        self.control()
    }

    fn master_brightness_control(&self) -> BrightnessControl {
        self.master_brightness_control()
    }

    fn master_brightness_factor(&self) -> u32 {
        self.master_brightness_factor()
    }

    fn bg_control(&self, i: BgIndex) -> BgControl {
        self.bgs[i.get() as usize].control()
    }

    fn bg_priority(&self, i: BgIndex) -> u8 {
        self.bgs[i.get() as usize].priority()
    }

    fn bg_scroll(&self, i: BgIndex) -> [u16; 2] {
        self.bgs[i.get() as usize].scroll
    }

    fn affine_bg_x_incr(&self, i: AffineBgIndex) -> [i16; 2] {
        self.affine_bg_data[i.get() as usize].x_incr
    }

    fn affine_bg_pos(&self, i: AffineBgIndex) -> [i32; 2] {
        self.affine_bg_data[i.get() as usize].pos
    }

    fn increase_affine_bg_pos(&mut self, i: AffineBgIndex) {
        let affine = &mut self.affine_bg_data[i.get() as usize];
        affine.pos = [
            affine.pos[0].wrapping_add(affine.y_incr[0] as i32),
            affine.pos[1].wrapping_add(affine.y_incr[1] as i32),
        ];
    }

    fn color_effects_control(&self) -> ColorEffectsControl {
        self.color_effects_control()
    }

    fn blend_coeffs(&self) -> (u8, u8) {
        self.blend_coeffs()
    }

    fn brightness_coeff(&self) -> u8 {
        self.brightness_coeff()
    }

    fn engine_3d_enabled_in_frame(&self) -> bool {
        self.engine_3d_enabled_in_frame()
    }
}

impl<R: Role, const BG_VRAM_LEN: usize, const OBJ_VRAM_LEN: usize>
    VramTrait<R, BG_VRAM_LEN, OBJ_VRAM_LEN> for Vram
{
    fn bg(&self) -> &Bytes<BG_VRAM_LEN> {
        assert!(BG_VRAM_LEN == R::BG_VRAM_LEN);
        unsafe {
            &*(if R::IS_A {
                self.a_bg.as_bytes_ptr() as *const ()
            } else {
                self.b_bg.as_bytes_ptr() as *const ()
            } as *const _)
        }
    }

    fn obj(&self) -> &Bytes<OBJ_VRAM_LEN> {
        assert!(OBJ_VRAM_LEN == R::OBJ_VRAM_LEN);
        unsafe {
            &*(if R::IS_A {
                self.a_obj.as_bytes_ptr() as *const ()
            } else {
                self.b_obj.as_bytes_ptr() as *const ()
            } as *const _)
        }
    }

    fn bg_palette(&self) -> &Bytes<0x206> {
        unsafe { &*((self.palette.as_ptr()).add((!R::IS_A as usize) << 10) as *const Bytes<0x206>) }
    }

    fn obj_palette(&self) -> &Bytes<0x206> {
        unsafe {
            &*(self.palette.as_ptr().add((!R::IS_A as usize) << 10 | 0x200) as *const Bytes<0x206>)
        }
    }

    fn bg_ext_palette(&self) -> &Bytes<0x8006> {
        unsafe {
            if R::IS_A {
                self.a_bg_ext_pal.as_bytes()
            } else {
                &*(self.b_bg_ext_pal_ptr as *const Bytes<0x8006>)
            }
        }
    }

    fn obj_ext_palette(&self) -> &Bytes<0x2006> {
        unsafe {
            if R::IS_A {
                self.a_obj_ext_pal.as_bytes()
            } else {
                &*(self.b_obj_ext_pal_ptr as *const Bytes<0x2006>)
            }
        }
    }

    fn oam(&self) -> &Bytes<0x400> {
        unsafe { &*(self.oam.as_ptr().add((!R::IS_A as usize) << 10) as *const Bytes<0x400>) }
    }
}
