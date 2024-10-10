#![allow(clippy::unused_unit)]
#![feature(new_zeroed_alloc)]

mod audio;
#[cfg(feature = "log")]
mod console_log;
pub mod renderer_3d;

use dust_core::{
    cpu::{self, arm7, arm9, interpreter::Interpreter},
    ds_slot,
    emu::{self, input::Keys, Emu},
    flash::Flash,
    gpu::{SCREEN_HEIGHT, SCREEN_WIDTH},
    rtc,
    spi::firmware,
    utils::{zeroed_box, BoxedByteSlice, Bytes},
    Model, SaveContents,
};
use js_sys::{Function, Uint32Array, Uint8Array};
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[wasm_bindgen]
pub enum SaveType {
    None,
    Eeprom4k,
    EepromFram64k,
    EepromFram512k,
    EepromFram1m,
    Flash2m,
    Flash4m,
    Flash8m,
    Nand64m,
    Nand128m,
    Nand256m,
}

impl SaveType {
    pub fn expected_len(self) -> Option<usize> {
        match self {
            SaveType::None => None,
            SaveType::Eeprom4k => Some(0x200),
            SaveType::EepromFram64k => Some(0x2000),
            SaveType::EepromFram512k => Some(0x1_0000),
            SaveType::EepromFram1m => Some(0x2_0000),
            SaveType::Flash2m => Some(0x4_0000),
            SaveType::Flash4m => Some(0x8_0000),
            SaveType::Flash8m => Some(0x10_0000),
            SaveType::Nand64m => Some(0x80_0000),
            SaveType::Nand128m => Some(0x100_0000),
            SaveType::Nand256m => Some(0x200_0000),
        }
    }

    pub fn from_save_len(len: usize) -> Option<Self> {
        match len {
            0x200 => Some(SaveType::Eeprom4k),
            0x2000 => Some(SaveType::EepromFram64k),
            0x1_0000 => Some(SaveType::EepromFram512k),
            0x2_0000 => Some(SaveType::EepromFram1m),
            0x4_0000 => Some(SaveType::Flash2m),
            0x8_0000 => Some(SaveType::Flash4m),
            0x10_0000 => Some(SaveType::Flash8m),
            0x80_0000 => Some(SaveType::Nand64m),
            0x100_0000 => Some(SaveType::Nand128m),
            0x200_0000 => Some(SaveType::Nand256m),
            _ => None,
        }
    }
}

#[wasm_bindgen]
pub enum WbgModel {
    Ds,
    Lite,
    Ique,
    IqueLite,
    Dsi,
}

impl From<WbgModel> for Model {
    fn from(other: WbgModel) -> Self {
        match other {
            WbgModel::Ds => Model::Ds,
            WbgModel::Lite => Model::Lite,
            WbgModel::Ique => Model::Ique,
            WbgModel::IqueLite => Model::IqueLite,
            WbgModel::Dsi => Model::Dsi,
        }
    }
}

#[wasm_bindgen]
pub struct EmuState {
    #[cfg(feature = "log")]
    logger: slog::Logger,
    model: Model,
    emu: Option<Emu<Interpreter>>,
    arm7_bios: Option<Box<Bytes<{ arm7::BIOS_SIZE }>>>,
    arm9_bios: Option<Box<Bytes<{ arm9::BIOS_SIZE }>>>,
}

fn build_emu<E: cpu::Engine>(emu_builder: emu::Builder, engine: E) -> emu::Emu<E> {
    match emu_builder.build(engine) {
        Ok(emu) => emu,
        Err(err) => match err {
            emu::BuildError::MissingRom => unreachable!("Missing DS slot ROM"),
            emu::BuildError::MissingSysFiles => unreachable!("Missing emulator system files"),
            emu::BuildError::RomCreation(err) => match err {
                ds_slot::rom::normal::CreationError::InvalidSize => {
                    unreachable!("Invalid DS slot ROM file size")
                }
            },
            emu::BuildError::RomNeedsDecryptionButNoBiosProvided => {
                panic!("Couldn't start emulator: ROM needs decryption but no BIOS provided.");
            }
        },
    }
}

// rust-analyzer needs this not to trigger a warning about generated function names
#[allow(non_snake_case)]
#[wasm_bindgen]
impl EmuState {
    pub fn reset(&mut self) {
        let emu = self.emu.take().unwrap();

        let (renderer_2d, renderer_3d_tx) = emu.gpu.into_renderers();

        let mut emu_builder = emu::Builder::new(
            emu.spi.firmware.reset(),
            emu.ds_slot.rom.into_contents(),
            emu.ds_slot.spi.reset(),
            emu.audio.backend,
            None,
            emu.rtc.backend,
            renderer_2d,
            renderer_3d_tx,
            None,
            #[cfg(feature = "log")]
            self.logger.clone(),
        );

        emu_builder.arm7_bios.clone_from(&self.arm7_bios);
        emu_builder.arm9_bios.clone_from(&self.arm9_bios);

        emu_builder.model = self.model;
        emu_builder.direct_boot = true;

        self.emu = Some(build_emu(emu_builder, Interpreter));
    }

    pub fn load_save(&mut self, ram_arr: Uint8Array) {
        ram_arr.copy_to(self.emu.as_mut().unwrap().ds_slot.spi.contents_mut())
    }

    pub fn export_save(&self) -> Uint8Array {
        Uint8Array::from(self.emu.as_ref().unwrap().ds_slot.spi.contents())
    }

    pub fn update_input(&mut self, pressed: u32, released: u32) {
        let emu = self.emu.as_mut().unwrap();
        emu.press_keys(Keys::from_bits_truncate(pressed));
        emu.release_keys(Keys::from_bits_truncate(released));
    }

    pub fn update_touch(&mut self, x: Option<u16>, y: Option<u16>) {
        let emu = self.emu.as_mut().unwrap();
        if let Some((x, y)) = x.zip(y) {
            emu.set_touch_pos([x, y]);
        } else {
            emu.end_touch();
        }
    }

    pub fn run_frame(&mut self) -> Uint32Array {
        // TODO: Handle an eventual shutdown
        let emu = self.emu.as_mut().unwrap();
        emu.run();
        Uint32Array::from(unsafe {
            core::slice::from_raw_parts(
                emu.gpu.renderer_2d().framebuffer().as_ptr() as *const u32,
                SCREEN_WIDTH * SCREEN_HEIGHT * 2,
            )
        })
    }
}

// Wasm-bindgen creates invalid output using a constructor, for some reason
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn create_emu_state(
    arm7_bios_arr: Option<Uint8Array>,
    arm9_bios_arr: Option<Uint8Array>,
    firmware_arr: Option<Uint8Array>,
    rom_arr: Uint8Array,
    save_contents_arr: Option<Uint8Array>,
    save_type: Option<SaveType>,
    has_ir: bool,
    model: WbgModel,
    audio_callback: Function,
) -> EmuState {
    console_error_panic_hook::set_once();

    #[cfg(feature = "log")]
    let logger = slog::Logger::root(console_log::Console::new(), slog::o!());

    let arm7_bios = arm7_bios_arr.map(|arr| {
        let mut buf = zeroed_box::<Bytes<{ arm7::BIOS_SIZE }>>();
        arr.copy_to(&mut **buf);
        buf
    });

    let arm9_bios = arm9_bios_arr.map(|arr| {
        let mut buf = zeroed_box::<Bytes<{ arm9::BIOS_SIZE }>>();
        arr.copy_to(&mut **buf);
        buf
    });

    let model = Model::from(model);

    let firmware = firmware_arr
        .map(|arr| {
            let mut buf = BoxedByteSlice::new_zeroed(arr.length() as usize);
            arr.copy_to(&mut buf);
            buf
        })
        .unwrap_or_else(|| firmware::default(model));

    let mut rom = BoxedByteSlice::new_zeroed(rom_arr.length().next_power_of_two() as usize);
    rom_arr.copy_to(&mut rom[..rom_arr.length() as usize]);
    if !ds_slot::rom::is_valid_size(rom.len() as u64, model) {
        panic!("Invalid ROM size");
    }

    let save_contents = save_contents_arr.map(|save_contents_arr| {
        let mut save_contents = BoxedByteSlice::new_zeroed(save_contents_arr.length() as usize);
        save_contents_arr.copy_to(&mut save_contents);
        save_contents
    });

    let ds_slot_spi = {
        let save_type = if let Some(save_contents) = &save_contents {
            if let Some(save_type) = save_type {
                let expected_len = save_type.expected_len();
                if expected_len != Some(save_contents.len()) {
                    let (chosen_save_type, _message) = if let Some(detected_save_type) =
                        SaveType::from_save_len(save_contents.len())
                    {
                        (detected_save_type, "existing save file")
                    } else {
                        (save_type, "database entry")
                    };
                    #[cfg(feature = "log")]
                    slog::error!(
                        logger,
                        "Unexpected save file size: expected {}, got {} B; respecting {}.",
                        if let Some(expected_len) = expected_len {
                            std::borrow::Cow::from(format!("{expected_len} B"))
                        } else {
                            "no file".into()
                        },
                        save_contents.len(),
                        _message,
                    );
                    chosen_save_type
                } else {
                    save_type
                }
            } else {
                #[allow(clippy::unnecessary_lazy_evaluations)]
                SaveType::from_save_len(save_contents.len()).unwrap_or_else(|| {
                    #[cfg(feature = "log")]
                    slog::error!(
                        logger,
                        "Unrecognized save file size ({} B) and no database entry found, \
                         defaulting to an empty save.",
                        save_contents.len()
                    );
                    SaveType::None
                })
            }
        } else {
            #[allow(clippy::unnecessary_lazy_evaluations)]
            save_type.unwrap_or_else(|| {
                #[cfg(feature = "log")]
                slog::error!(
                    logger,
                    "No existing save file present and no database entry found, defaulting to an \
                     empty save.",
                );
                SaveType::None
            })
        };

        if save_type == SaveType::None {
            ds_slot::spi::Empty::new(
                #[cfg(feature = "log")]
                logger.new(slog::o!("ds_spi" => "empty")),
            )
            .into()
        } else {
            let expected_len = save_type.expected_len().unwrap();
            let save_contents = match save_contents {
                Some(save_contents) => {
                    SaveContents::Existing(if save_contents.len() != expected_len {
                        let mut new_contents = BoxedByteSlice::new_zeroed(expected_len);
                        let copy_len = save_contents.len().min(expected_len);
                        new_contents[..copy_len].copy_from_slice(&save_contents[..copy_len]);
                        new_contents
                    } else {
                        save_contents
                    })
                }
                None => SaveContents::New(expected_len),
            };
            match save_type {
                SaveType::None => unreachable!(),
                SaveType::Eeprom4k => ds_slot::spi::eeprom_4k::Eeprom4k::new(
                    save_contents,
                    None,
                    #[cfg(feature = "log")]
                    logger.new(slog::o!("ds_spi" => "eeprom_4k")),
                )
                // NOTE: The save contents' size is ensured beforehand, this should never occur.
                .expect("couldn't create 4 Kib EEPROM DS slot SPI device")
                .into(),
                SaveType::EepromFram64k | SaveType::EepromFram512k | SaveType::EepromFram1m => {
                    ds_slot::spi::eeprom_fram::EepromFram::new(
                        save_contents,
                        None,
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => "eeprom_fram")),
                    )
                    // NOTE: The save contents' size is ensured beforehand, this should never occur.
                    .expect("couldn't create EEPROM/FRAM DS slot SPI device")
                    .into()
                }
                SaveType::Flash2m | SaveType::Flash4m | SaveType::Flash8m => {
                    ds_slot::spi::flash::Flash::new(
                        save_contents,
                        [0; 20],
                        has_ir,
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => if has_ir { "flash" } else { "flash_ir" })),
                    )
                    // NOTE: The save contents' size is ensured beforehand, this should never occur.
                    .expect("couldn't create FLASH DS slot SPI device")
                    .into()
                }
                SaveType::Nand64m | SaveType::Nand128m | SaveType::Nand256m => {
                    #[cfg(feature = "log")]
                    slog::error!(
                        logger,
                        "TODO: NAND saves are currently unsupported, falling back to no save file."
                    );
                    ds_slot::spi::Empty::new(
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => "nand_todo")),
                    )
                    .into()
                }
            }
        }
    };

    let (tx_3d, rx_3d) = renderer_3d::init();

    let mut emu_builder = emu::Builder::new(
        Flash::new(
            SaveContents::Existing(firmware),
            firmware::id_for_model(model),
            #[cfg(feature = "log")]
            logger.new(slog::o!("fw" => "")),
        )
        .expect("couldn't build firmware"),
        Some(Box::new(rom)),
        ds_slot_spi,
        Box::new(audio::Backend::new(audio_callback)),
        None,
        Box::new(rtc::DummyBackend),
        Box::new(dust_soft_2d::sync::Renderer::new(Box::new(rx_3d))),
        Box::new(tx_3d),
        None,
        #[cfg(feature = "log")]
        logger.clone(),
    );

    emu_builder.arm7_bios.clone_from(&arm7_bios);
    emu_builder.arm9_bios.clone_from(&arm9_bios);

    emu_builder.model = model;
    emu_builder.direct_boot = true;

    let emu = build_emu(emu_builder, Interpreter);

    EmuState {
        #[cfg(feature = "log")]
        logger,
        model,
        emu: Some(emu),
        arm7_bios,
        arm9_bios,
    }
}

#[wasm_bindgen]
pub fn internal_get_module() -> wasm_bindgen::JsValue {
    wasm_bindgen::module()
}

#[wasm_bindgen]
pub fn internal_get_memory() -> wasm_bindgen::JsValue {
    wasm_bindgen::memory()
}
