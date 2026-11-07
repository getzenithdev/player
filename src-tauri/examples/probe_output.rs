fn main() {
    println!(
        "Rodio default: {:?}",
        rodio::DeviceSinkBuilder::open_default_sink().map(|mut sink| {
            sink.log_on_drop(false);
            format!("{:?}", sink.config())
        })
    );
    println!(
        "System buffer: {:?}",
        rodio::DeviceSinkBuilder::from_default_device()
            .and_then(|builder| builder
                .with_buffer_size(rodio::cpal::BufferSize::Default)
                .open_stream())
            .map(|mut sink| {
                sink.log_on_drop(false);
                format!("{:?}", sink.config())
            })
    );
}
