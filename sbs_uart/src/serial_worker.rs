use std::io::ErrorKind;
use std::thread;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::mpsc::error::{SendError, TryRecvError};
use tokio;
use std::time::Duration;
use serialport::{ClearBuffer, SerialPort};
use tokio::time::error::Elapsed;
use tokio::time::timeout;
use crate::error::Error;
use crate::frame_decoder::{DecodedFrame, Decoder, DecodeResult, FrameDetails, FrameInfo, RawSignalFrame};

#[derive(Clone, Debug)]
enum CommandReq {
    Connect(String, u32),
    Disconnect,
    Stop,
    ListFrames,
    GetFrameInfo(u32),
    EnableFrame(u32),
    DisableFrame(u32),
}


impl<T> From<SendError<T>> for Error {
    fn from(value: SendError<T>) -> Self {
        Error::Internal(format!("Failed to send to channel: {value:?}"))
    }
}


#[derive(Clone, Debug)]
enum CommandRes {
    Connect(Result<(), Error>),
    Disconnect(Result<(), Error>),
    ListFrames(Result<Vec<FrameInfo>, Error>),
    GetFrameInfo(Result<FrameDetails, Error>),
    EnableFrame(Result<(), Error>),
    DisableFrame(Result<(), Error>),
    Error(Error),
}

pub struct SerialWorker {
    txchan_tx: Sender<CommandReq>,
    rxchan_rx: Receiver<CommandRes>,
    reader_thread: thread::JoinHandle<()>,
}

impl SerialWorker {
    pub fn new(raw_frame_tx: Sender<RawSignalFrame>) -> SerialWorker {
        let (txchan_tx, txchan_rx): (Sender<CommandReq>, Receiver<CommandReq>) = mpsc::channel(16);
        let (rxchan_tx, rxchan_rx): (Sender<CommandRes>, Receiver<CommandRes>) = mpsc::channel(16);


        SerialWorker {
            txchan_tx,
            rxchan_rx,
            reader_thread: thread::spawn(move || {
                let mut worker = SerialWorkerThread::new(txchan_rx, rxchan_tx, raw_frame_tx);
                worker.run();
            }),
        }
    }

    pub async fn connect(&mut self, port: &str, baud: u32) -> Result<(), Error> {
        match self.request(CommandReq::Connect(port.to_string(), baud), Duration::from_millis(2000)).await? {
            CommandRes::Connect(r) => r,
            CommandRes::Error(e) => Err(e),
            res => Err(Error::Internal(format!("Invalid response from worker {res:?}")))
        }
    }

    pub async fn list_frames(&mut self) -> Result<Vec<FrameInfo>, Error> {
        match self.request(CommandReq::ListFrames, Duration::from_millis(2000)).await? {
            CommandRes::ListFrames(r) => r,
            CommandRes::Error(e) => Err(e),
            res => Err(Error::Internal(format!("Invalid response from worker {res:?}")))
        }
    }

    pub async fn get_frame_info(&mut self, frame_id: u32) -> Result<FrameDetails, Error> {
        match self.request(CommandReq::GetFrameInfo(frame_id), Duration::from_millis(2000)).await? {
            CommandRes::GetFrameInfo(r) => r,
            CommandRes::Error(e) => Err(e),
            res => Err(Error::Internal(format!("Invalid response from worker {res:?}")))
        }
    }

    pub async fn enable_frame(&mut self, frame_id: u32) -> Result<(), Error> {
        match self.request(CommandReq::EnableFrame(frame_id), Duration::from_millis(2000)).await? {
            CommandRes::EnableFrame(r) => r,
            CommandRes::Error(e) => Err(e),
            res => Err(Error::Internal(format!("Invalid response from worker {res:?}")))
        }
    }

    pub async fn disable_frame(&mut self, frame_id: u32) -> Result<(), Error> {
        match self.request(CommandReq::DisableFrame(frame_id), Duration::from_millis(2000)).await? {
            CommandRes::DisableFrame(r) => r,
            CommandRes::Error(e) => Err(e),
            res => Err(Error::Internal(format!("Invalid response from worker {res:?}")))
        }
    }

    pub async fn quit(self) -> Result<(), Error> {
        self.txchan_tx.send(CommandReq::Stop).await?;

        self.reader_thread.join().unwrap();

        Ok(())
    }

    async fn request(&mut self, req: CommandReq, to: Duration) -> Result<CommandRes, Error> {
        self.txchan_tx.send(req).await?;

        timeout(to, self.rxchan_rx.recv()).await?
            .ok_or(Error::Internal("Failed to receive".to_string()))
    }
}

#[derive(Clone, Debug)]
enum WorkerState {
    Disconnected,
    Connected,
    ListFrames,
    GetFrameInfo,
    EnableFrame,
    DisableFrame,
}

struct SerialWorkerThread {
    txchan_rx: Receiver<CommandReq>,
    rxchan_tx: Sender<CommandRes>,
    raw_frame_tx: Sender<RawSignalFrame>,
    state: WorkerState,
    quit: bool,
    serial: Option<Box<dyn SerialPort>>,
    decoder: Decoder,
}

impl SerialWorkerThread {
    fn new(txchan_rx: Receiver<CommandReq>,
           rxchan_tx: Sender<CommandRes>,
           raw_frame_tx: Sender<RawSignalFrame>) -> SerialWorkerThread {
        SerialWorkerThread {
            txchan_rx,
            rxchan_tx,
            raw_frame_tx,
            state: WorkerState::Disconnected,
            quit: false,
            serial: None,
            decoder: Decoder::new(),
        }
    }

    fn run(&mut self) {
        loop {
            let current_state = self.state.clone();

            let new_state = match current_state {
                WorkerState::Disconnected => self.handle_disconnected_state(),
                WorkerState::Connected => self.handle_connected_state(),
                WorkerState::ListFrames => self.handle_list_frames_state(),
                WorkerState::GetFrameInfo => self.handle_get_frame_info_state(),
                WorkerState::EnableFrame => self.handle_enable_frame_state(),
                WorkerState::DisableFrame => self.handle_disable_frame_state(),
            };

            self.state = new_state.unwrap_or(current_state);

            if self.quit {
                break;
            }
        }
    }

    fn handle_disconnected_state(&mut self) -> Option<WorkerState> {
        match self.txchan_rx.blocking_recv() {
            Some(CommandReq::Connect(port_name, baud)) => {
                let port_builder = serialport::new(port_name, baud).timeout(Duration::from_millis(100));

                match port_builder.open().and_then(|port| {
                    port.clear(ClearBuffer::All)?;
                    Ok(port)
                }) {
                    Ok(port) => {
                        self.serial = Some(port);
                        self.decoder = Decoder::new();

                        self.send_response(CommandRes::Connect(Ok(())));
                        Some(WorkerState::Connected)
                    }
                    Err(err) => {
                        self.send_response(CommandRes::Connect(Err(Error::SerialError(format!("Failed to open serial port: {err}")))));
                        None
                    }
                }
            }
            Some(CommandReq::Stop) => {
                self.quit = true;
                None
            }
            Some(cmd) => {
                self.send_response(CommandRes::Error(Error::InvalidCommand(format!("Invalid command {cmd:?}"))));
                None
            }
            None => {
                println!("Failed to receive command");
                None
            }
        }
    }

    fn handle_connected_state(&mut self) -> Option<WorkerState> {
        let mut serial_buf: Vec<u8> = vec![0; 2048];

        let cmd_result = match self.txchan_rx.try_recv() {
            Ok(CommandReq::Disconnect) => {
                self.serial = None;
                Some(WorkerState::Disconnected)
            }
            Ok(CommandReq::ListFrames) => {
                let ser = self.serial.as_mut().unwrap();
                match ser.write(b"lL") {
                    Ok(_) => Some(WorkerState::ListFrames),
                    Err(e) => {
                        self.send_response(CommandRes::ListFrames(Err(Error::SerialError(format!("Failed to send data: {e:?}")))));
                        None
                    }
                }
            }
            Ok(CommandReq::GetFrameInfo(frame_id)) => {
                let fid_bytes = frame_id.to_le_bytes();
                let mut tx_buf: [u8; 6] = [b'i', 0, 0, 0, 0, b'I'];
                tx_buf.as_mut_slice()[1..5].copy_from_slice(&fid_bytes);

                let ser = self.serial.as_mut().unwrap();
                match ser.write(tx_buf.as_slice()) {
                    Ok(_) => Some(WorkerState::GetFrameInfo),
                    Err(e) => {
                        self.send_response(CommandRes::GetFrameInfo(Err(Error::SerialError(format!("Failed to send data: {e:?}")))));
                        None
                    }
                }
            }
            Ok(CommandReq::EnableFrame(frame_id)) => {
                let fid_bytes = frame_id.to_le_bytes();
                let mut tx_buf: [u8; 6] = [b'e', 0, 0, 0, 0, b'E'];
                tx_buf.as_mut_slice()[1..5].copy_from_slice(&fid_bytes);

                let ser = self.serial.as_mut().unwrap();
                match ser.write(tx_buf.as_slice()) {
                    Ok(_) => Some(WorkerState::EnableFrame),
                    Err(e) => {
                        self.send_response(CommandRes::EnableFrame(Err(Error::SerialError(format!("Failed to send data: {e:?}")))));
                        None
                    }
                }
            }
            Ok(CommandReq::DisableFrame(frame_id)) => {
                let fid_bytes = frame_id.to_le_bytes();
                let mut tx_buf: [u8; 6] = [b'd', 0, 0, 0, 0, b'D'];
                tx_buf.as_mut_slice()[1..5].copy_from_slice(&fid_bytes);

                let ser = self.serial.as_mut().unwrap();
                match ser.write(tx_buf.as_slice()) {
                    Ok(_) => Some(WorkerState::DisableFrame),
                    Err(e) => {
                        self.send_response(CommandRes::DisableFrame(Err(Error::SerialError(format!("Failed to send data: {e:?}")))));
                        None
                    }
                }
            }
            Ok(CommandReq::Stop) => {
                self.quit = true;
                None
            }
            Ok(cmd) => {
                self.send_response(CommandRes::Error(Error::InvalidCommand(format!("Invalid command {cmd:?}"))));
                None
            }
            Err(TryRecvError::Empty) => None,
            Err(err) => {
                self.send_response(CommandRes::ListFrames(Err(Error::SerialError(format!("Failed to receive data: {err:?}")))));
                None
            }
        };

        if cmd_result.is_some() {
            return cmd_result;
        }

        let ser = self.serial.as_mut().unwrap();
        match ser.read(serial_buf.as_mut_slice()) {
            Ok(nb) => {
                loop {
                    match self.decoder.decode(&serial_buf.as_slice()[..nb]) {
                        DecodeResult::None => break,
                        DecodeResult::CmdFrame(frame) =>
                            self.send_response(CommandRes::GetFrameInfo(Err(Error::WrongFrame(format!("Unexpected frame {frame:?}"))))),
                        DecodeResult::Err(err) =>
                            self.send_response(CommandRes::GetFrameInfo(Err(Error::DecodeError(err)))),
                        DecodeResult::SignalFrame(rsf) =>
                            self.send_signal_frame(rsf),
                    };
                }


                None
            }
            Err(err) if err.kind() == ErrorKind::TimedOut => None,
            Err(err) => {
                self.send_response(CommandRes::GetFrameInfo(Err(Error::SerialError(format!("Failed to read from serial: {err:?}")))));
                Some(WorkerState::Connected)
            }
        }
    }

    fn handle_list_frames_state(&mut self) -> Option<WorkerState> {
        self.read_response(|frame| match frame {
            DecodedFrame::ListFrames(frames) =>
                CommandRes::ListFrames(Ok(frames)),
            frame =>
                CommandRes::ListFrames(Err(Error::WrongFrame(format!("Wrong response frame, expected ListFrames, got {frame:?}")))),
        })
    }

    fn handle_get_frame_info_state(&mut self) -> Option<WorkerState> {
        self.read_response(|frame| match frame {
            DecodedFrame::GetFrameInfo(details) =>
                CommandRes::GetFrameInfo(Ok(details)),
            frame =>
                CommandRes::ListFrames(Err(Error::WrongFrame(format!("Wrong response frame, expected GetFrameInfo, got {frame:?}")))),
        })
    }

    fn handle_enable_frame_state(&mut self) -> Option<WorkerState> {
        self.read_response(|frame| match frame {
            DecodedFrame::EnableFrame =>
                CommandRes::EnableFrame(Ok(())),
            frame =>
                CommandRes::ListFrames(Err(Error::WrongFrame(format!("Wrong response frame, expected GetFrameInfo, got {frame:?}")))),
        })
    }

    fn handle_disable_frame_state(&mut self) -> Option<WorkerState> {
        self.read_response(|frame| match frame {
            DecodedFrame::DisableFrame =>
                CommandRes::DisableFrame(Ok(())),
            frame =>
                CommandRes::ListFrames(Err(Error::WrongFrame(format!("Wrong response frame, expected DisableFrame, got {frame:?}")))),
        })
    }

    fn read_response<F>(&mut self, map_frame: F) -> Option<WorkerState>
        where F: FnOnce(DecodedFrame) -> CommandRes {
        let mut serial_buf: Vec<u8> = vec![0; 32];
        let ser = self.serial.as_mut().unwrap();

        match ser.read(serial_buf.as_mut_slice()) {
            Ok(nb) => loop {
                match self.decoder.decode(&serial_buf.as_slice()[..nb]) {
                    DecodeResult::None => return None,
                    DecodeResult::CmdFrame(frame) => {
                        self.send_response(map_frame(frame));
                        return Some(WorkerState::Connected);
                    }
                    DecodeResult::Err(err) => {
                        self.send_response(CommandRes::Error(Error::DecodeError(err)));
                        return Some(WorkerState::Connected);
                    }
                    DecodeResult::SignalFrame(rsf) => self.send_signal_frame(rsf),
                }
            }
            Err(err) if err.kind() == ErrorKind::TimedOut => None,
            Err(err) => {
                self.send_response(CommandRes::Error(Error::SerialError(format!("Failed to read from serial: {err:?}"))));
                Some(WorkerState::Connected)
            }
        }
    }

    fn send_response(&mut self, msg: CommandRes) {
        if let Err(send_err) = self.rxchan_tx.blocking_send(msg) {
            println!("Failed to send response: {send_err:?}");
        }
    }

    fn send_signal_frame(&mut self, rsf: RawSignalFrame) {
        if let Err(send_err) = self.raw_frame_tx.try_send(rsf) {
            println!("Failed to send signal frame: {send_err:?}");
        }
    }
}

impl From<Elapsed> for Error {
    fn from(value: Elapsed) -> Self {
        Error::Timeout
    }
}
