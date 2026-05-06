use crate::platform::Instant;

#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
pub(crate) fn notify_after_metal_queue_completion(
    queue: &wgpu::Queue,
    callback: Box<dyn FnOnce(Instant) + Send>,
) -> Result<(), Box<dyn FnOnce(Instant) + Send>> {
    let Some(hal_queue) = (unsafe { queue.as_hal::<wgpu::hal::api::Metal>() }) else {
        return Err(callback);
    };
    let raw_queue = hal_queue.as_raw().lock();
    let command_buffer = raw_queue.new_command_buffer();
    command_buffer.set_label("floem gpu completion sentinel");

    let callback = Arc::new(Mutex::new(Some(callback)));
    let block_callback = callback.clone();
    let block = block::ConcreteBlock::new(move |_| {
        if let Some(callback) = block_callback.lock().unwrap().take() {
            callback(Instant::now());
        }
    })
    .copy();
    command_buffer.add_completed_handler(&block);
    command_buffer.commit();
    Ok(())
}
