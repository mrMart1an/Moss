use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use nvml_wrapper::Nvml;
use tokio::{
    select,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::{device::GpuDevice, gpus_manager::gpu_data::DeviceData};

pub mod gpu_data;

type Responder = oneshot::Sender<GpusManagerAnswer>;

pub enum GpusManagerMessage {
    // Getters
    GetName { uuid: String, tx: Responder },

    GetDriverVersion { uuid: String, tx: Responder },
    GetVbios { uuid: String, tx: Responder },
    GetNumCores { uuid: String, tx: Responder },
    GetPcieWidth { uuid: String, tx: Responder },
    GetPcieGen { uuid: String, tx: Responder },

    GetTemp { uuid: String, tx: Responder },

    GetGraphicsFreq { uuid: String, tx: Responder },
    GetVideoFreq { uuid: String, tx: Responder },
    GetSmFreq { uuid: String, tx: Responder },
    GetMemFreq { uuid: String, tx: Responder },

    GetGraphicsBoostFreq { uuid: String, tx: Responder },
    GetVideoBoostFreq { uuid: String, tx: Responder },
    GetSmBoostFreq { uuid: String, tx: Responder },
    GetMemBoostFreq { uuid: String, tx: Responder },

    GetCoreClockOffset { uuid: String, tx: Responder },
    GetMemClockOffset { uuid: String, tx: Responder },

    GetPowerUsage { uuid: String, tx: Responder },
    GetPowerLimit { uuid: String, tx: Responder },
    GetPowerLimitMax { uuid: String, tx: Responder },
    GetPowerLimitMin { uuid: String, tx: Responder },
    GetPowerDefault { uuid: String, tx: Responder },

    GetFanSpeed { uuid: String, tx: Responder },
    GetFanRpm { uuid: String, tx: Responder },

    GetCoreUsage { uuid: String, tx: Responder },
    GetMemUsage { uuid: String, tx: Responder },

    GetTotalMemory { uuid: String, tx: Responder },
    GetUsedMemory { uuid: String, tx: Responder },
    GetFreeMemory { uuid: String, tx: Responder },

    GetMaxTemp { uuid: String, tx: Responder },
    GetMemMaxTemp { uuid: String, tx: Responder },
    GetSlowdownTemp { uuid: String, tx: Responder },
    GetShutdownTemp { uuid: String, tx: Responder },

    // Setters
    SetCoreClockOffset { uuid: String, offset: i32 },
    SetMemClockOffset { uuid: String, offset: i32 },

    SetPowerLimit { uuid: String, limit: u32 },

    SetUpdateRate(Duration),
}

#[derive(Debug)]
pub enum GpusManagerAnswer {
    Name(String),

    DriverVersion(String),
    Vbios(String),
    NumCores(Option<u32>),
    PcieWidth(Option<u32>),
    PcieGen(Option<u32>),

    Temp(Option<u32>),

    GraphicsFreq(Option<u32>),
    VideoFreq(Option<u32>),
    SmFreq(Option<u32>),
    MemFreq(Option<u32>),

    GraphicsBoostFreq(Option<u32>),
    VideoBoostFreq(Option<u32>),
    SmBoostFreq(Option<u32>),
    MemBoostFreq(Option<u32>),

    CoreClockOffset(Option<i32>),
    MemClockOffset(Option<i32>),

    PowerUsage(Option<u32>),
    PowerLimit(Option<u32>),
    PowerLimitMax(u32),
    PowerLimitMin(u32),
    PowerDefault(u32),

    FanSpeed(Option<u32>),
    FanRpm(Option<u32>),

    CoreUsage(Option<u32>),
    MemUsage(Option<u32>),

    TotalMemory(Option<u64>),
    UsedMemory(Option<u64>),
    FreeMemory(Option<u64>),

    MaxTemp(Option<u32>),
    MemMaxTemp(Option<u32>),
    SlowdownTemp(Option<u32>),
    ShutdownTemp(Option<u32>),
}

// This object get and set properties to the system GPUs
pub struct GpusManager {
    nvml: Arc<Nvml>,

    devices: HashMap<String, GpuDevice>,
    device_datas: HashMap<String, DeviceData>,

    update_rate: Duration,
}

impl GpusManager {
    pub fn new(nvml: &Arc<Nvml>) -> Self {
        Self {
            nvml: nvml.clone(),
            devices: HashMap::new(),
            device_datas: HashMap::new(),

            update_rate: Duration::from_secs_f64(1.),
        }
    }

    pub async fn run(
        &mut self,
        run_token: CancellationToken,
        mut rx_message: Receiver<GpusManagerMessage>,
        tx_err: Sender<anyhow::Error>,
    ) {
        loop {
            select! {
                _ = run_token.cancelled() => {
                    info!("GPUs manager: Quiting");

                    break;
                },
                message = rx_message.recv() => {
                    if let Err(err) = self.parse_message(message) {
                        tx_err.send(err).await.unwrap_or_else(|err| {
                            error!("Failed to send error over channel: {err}");
                        });
                    }
                }
            }
        }
    }

    fn parse_message(
        &mut self,
        message: Option<GpusManagerMessage>,
    ) -> Result<()> {
        if message.is_none() {
            warn!("GPUs manager: parsing empty message");
            return Ok(());
        }

        match message.unwrap() {
            // Getters
            GpusManagerMessage::GetName { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::Name(data.name.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetDriverVersion { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::DriverVersion(
                    data.driver_version.clone(),
                );

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetVbios { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::Vbios(data.vbios.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetNumCores { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::NumCores(data.num_cores.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetPcieWidth { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::PcieWidth(data.pcie_width.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetPcieGen { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::PcieGen(data.pcie_gen.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetTemp { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::Temp(data.temp_gpu.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetGraphicsFreq { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::GraphicsFreq(data.graphics_freq.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetVideoFreq { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::VideoFreq(data.video_freq.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetSmFreq { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::SmFreq(data.sm_freq.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetMemFreq { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::MemFreq(data.mem_freq.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetGraphicsBoostFreq { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::GraphicsBoostFreq(
                    data.graphics_boost_freq.clone(),
                );

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetVideoBoostFreq { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::VideoBoostFreq(
                    data.video_boost_freq.clone(),
                );

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetSmBoostFreq { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::SmBoostFreq(data.sm_boost_freq.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetMemBoostFreq { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::MemBoostFreq(
                    data.mem_boost_freq.clone(),
                );

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetCoreClockOffset { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::CoreClockOffset(
                    data.core_clock_offset.clone(),
                );

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetMemClockOffset { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::MemClockOffset(
                    data.mem_clock_offset.clone(),
                );

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetPowerUsage { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::PowerUsage(data.power_usage.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetPowerLimit { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::PowerLimit(data.power_limit.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetPowerLimitMax { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::PowerLimitMax(
                    data.power_limit_max.clone(),
                );

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetPowerLimitMin { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::PowerLimitMin(
                    data.power_limit_min.clone(),
                );

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetPowerDefault { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::PowerDefault(
                    data.power_limit_default.clone(),
                );

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetFanSpeed { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::FanSpeed(data.fan_speed.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetFanRpm { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::FanRpm(data.fan_speed_rpm.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetCoreUsage { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::CoreUsage(data.core_usage.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetMemUsage { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::MemUsage(data.mem_usage.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetTotalMemory { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::TotalMemory(data.total_memory.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetUsedMemory { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::UsedMemory(data.used_memory.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetFreeMemory { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::FreeMemory(data.free_memory.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            GpusManagerMessage::GetMaxTemp { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer = GpusManagerAnswer::MaxTemp(data.max_temp.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetMemMaxTemp { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::MemMaxTemp(data.mem_max_temp.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetSlowdownTemp { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::SlowdownTemp(data.slowdown_temp.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }
            GpusManagerMessage::GetShutdownTemp { uuid, tx } => {
                let data = self.get_updated_data(uuid.as_str())?;
                let answer =
                    GpusManagerAnswer::ShutdownTemp(data.shutdown_temp.clone());

                tx.send(answer).map_err(|v| anyhow!("{v:?}"))?;
            }

            // Setters
            GpusManagerMessage::SetCoreClockOffset { uuid, offset } => {
                let device = self.get_device(uuid.as_str())?.get()?;
                device.set_gpc_clock_vf_offset(offset)?;
            }
            GpusManagerMessage::SetMemClockOffset { uuid, offset } => {
                let device = self.get_device(uuid.as_str())?.get()?;
                device.set_mem_clock_vf_offset(offset)?;
            }

            GpusManagerMessage::SetPowerLimit { uuid, limit } => {
                let mut device = self.get_device(uuid.as_str())?.get()?;
                device.set_power_management_limit(limit)?;
            }

            GpusManagerMessage::SetUpdateRate(duration) => {
                self.update_rate = duration;
            }
        }

        Ok(())
    }

    fn get_device(&mut self, uuid: &str) -> Result<&GpuDevice> {
        if self.devices.contains_key(uuid) {
            self.devices
                .get(uuid)
                .ok_or_else(|| anyhow!("Key not found in hash map"))
        } else {
            // If the device isn't in the hash map add it
            let device = GpuDevice::new(&self.nvml, uuid);
            self.devices.insert(uuid.to_string(), device);

            self.devices
                .get(uuid)
                .ok_or_else(|| anyhow!("Key not found in hash map"))
        }
    }

    fn get_updated_data(&mut self, uuid: &str) -> Result<&mut DeviceData> {
        if self.device_datas.contains_key(uuid) {
            let data = self
                .device_datas
                .get_mut(uuid)
                .ok_or_else(|| anyhow!("Key not found in hash map"))?;

            // Update the data if necessary
            data.update(self.update_rate);

            Ok(data)
        } else {
            // If the device isn't in the hash map add it
            let device = self.get_device(uuid)?;
            let device_data = DeviceData::new(&device)?;
            self.device_datas.insert(uuid.to_string(), device_data);

            let data = self
                .device_datas
                .get_mut(uuid)
                .ok_or_else(|| anyhow!("Key not found in hash map"))?;

            // Update the data if necessary
            data.update(self.update_rate);

            Ok(data)
        }
    }
}
