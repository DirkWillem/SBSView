use std::fmt::{Display};
use std::time::Duration;
use sbs_core::sbs::{Client};
use sbs_uart::sbs_uart::SbsUart;


#[tokio::main]
async fn main() {
    let mut client = SbsUart::new();
    client.connect("/dev/tty.usbmodem21103", 115_200).await.unwrap();

    let frames = client.get_frames().await.unwrap();


    for frame in &frames {
        client.enable_frame(frame.id).await.unwrap();
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    for frame in &frames {
        for i in 0..3 {
            match client.disable_frame(frame.id).await {
                Ok(_) => break,
                Err(e) => println!("error {e:?}, try {i}"),
            }
        }
    }

    client.close().await.unwrap();

}
