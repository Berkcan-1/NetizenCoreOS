use alloc::boxed::Box;
use alloc::vec::Vec;
use nc_hal::{BlockDevice, BlockDeviceInfo, InputDevice, KeyCode, MouseState};

pub struct DeviceManager {
    block_devices: Vec<Box<dyn BlockDevice>>,
    cached_block_infos: Vec<BlockDeviceInfo>,
    input_devices: Vec<Box<dyn InputDevice>>,
    last_mouse: Option<MouseState>,
}

impl DeviceManager {
    pub fn new() -> DeviceManager {
        DeviceManager {
            block_devices: Vec::new(),
            cached_block_infos: Vec::new(),
            input_devices: Vec::new(),
            last_mouse: None,
        }
    }

    pub fn register_block_device(&mut self, device: Box<dyn BlockDevice>) {
        self.cached_block_infos.push(device.info());
        self.block_devices.push(device);
    }

    #[allow(dead_code)]
    pub fn block_count(&self) -> usize {
        self.block_devices.len()
    }

    #[allow(dead_code)]
    pub fn block_device(&mut self, index: usize) -> Option<&mut (dyn BlockDevice + 'static)> {
        self.block_devices.get_mut(index).map(|d| d.as_mut())
    }

    #[inline]
    pub fn block_infos(&self) -> &[BlockDeviceInfo] {
        &self.cached_block_infos
    }

    pub fn register_input_device(&mut self, input: Box<dyn InputDevice>) {
        self.input_devices.push(input);
    }

    pub fn poll_input(&mut self, width: i32, height: i32) -> (MouseState, Option<KeyCode>) {
        let mut combined = self.last_mouse.unwrap_or(MouseState {
            x: width / 2,
            y: height / 2,
            left_down: false,
            left_clicked: false,
            right_down: false,
            right_clicked: false,
        });
        combined.left_clicked = false;
        combined.right_clicked = false;

        let mut any_left_down = false;
        let mut any_right_down = false;
        let mut key = None;

        for input in &mut self.input_devices {
            let (m, k) = input.poll(width, height);
            if m.x != combined.x || m.y != combined.y {
                combined.x = m.x;
                combined.y = m.y;
            }
            any_left_down |= m.left_down;
            any_right_down |= m.right_down;
            combined.left_clicked |= m.left_clicked;
            combined.right_clicked |= m.right_clicked;
            if k.is_some() {
                key = k;
            }
        }

        combined.left_down = any_left_down;
        combined.right_down = any_right_down;
        self.last_mouse = Some(combined);
        (combined, key)
    }
}
