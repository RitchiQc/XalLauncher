use crate::error::Result;
use crate::models::{Instance, ModuleState};
use crate::options;
use crate::store::Store;
use std::fs;
use std::path::Path;

const PERFORMANCE_FLAGS: &[&str] = &[
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:MaxGCPauseMillis=200",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+DisableExplicitGC",
    "-XX:+AlwaysPreTouch",
    "-XX:G1NewSizePercent=30",
    "-XX:G1MaxNewSizePercent=40",
    "-XX:G1HeapRegionSize=8M",
    "-XX:G1ReservePercent=20",
    "-XX:G1HeapWastePercent=5",
    "-XX:G1MixedGCCountTarget=4",
    "-XX:InitiatingHeapOccupancyPercent=15",
    "-XX:G1MixedGCLiveThresholdPercent=90",
    "-XX:G1RSetUpdatingPauseTimePercent=5",
    "-XX:SurvivorRatio=32",
    "-XX:+PerfDisableSharedMem",
    "-XX:MaxTenuringThreshold=1",
    "-Dusing.aikars.flags=https://mcflags.emc.gs",
    "-Daikars.new.flags=true",
];

const PERFORMANCE_OPTIONS: &[(&str, &str)] = &[
    ("renderDistance", "8"),
    ("simulationDistance", "6"),
    ("graphicsMode", "0"),
    ("particles", "2"),
    ("entityShadows", "false"),
    ("biomeBlendRadius", "0"),
    ("mipmapLevels", "0"),
    ("enableVsync", "false"),
    ("maxFps", "260"),
    ("ao", "true"),
    ("renderClouds", "false"),
    ("cloudStatus", "0"),
    ("syncChunkWrites", "false"),
];

pub fn apply_performance_jvm(jvm: &mut Vec<String>, cores: Option<usize>) {
    for flag in PERFORMANCE_FLAGS {
        if !jvm.iter().any(|argument| argument == flag) {
            jvm.push((*flag).to_string());
        }
    }
    let xmx = jvm
        .iter()
        .find(|argument| argument.starts_with("-Xmx"))
        .cloned();
    if let Some(xmx) = xmx {
        if let Some(xms) = jvm
            .iter_mut()
            .find(|argument| argument.starts_with("-Xms"))
        {
            *xms = xmx.replacen("-Xmx", "-Xms", 1);
        }
    }
    if let Some(cores) = cores {
        if !jvm
            .iter()
            .any(|argument| argument.starts_with("-XX:ActiveProcessorCount="))
        {
            jvm.push(format!("-XX:ActiveProcessorCount={cores}"));
        }
    }
}

fn apply_performance_options(game_dir: &Path) -> Result<()> {
    let path = game_dir.join("options.txt");
    if !path.exists() {
        return Ok(());
    }
    options::backup_once(&path)?;
    let mut entries = options::read(&path)?;
    for (key, value) in PERFORMANCE_OPTIONS {
        if options::get(&entries, key).is_some() {
            options::set(&mut entries, key, (*value).to_string());
        }
    }
    options::write(&path, &entries)
}

fn prepare_options(game_dir: &Path, enabled: bool) -> Result<()> {
    let marker = game_dir.join(".xal-performance-options");
    if enabled {
        apply_performance_options(game_dir)?;
        fs::write(marker, b"1")?;
    } else if marker.exists() {
        options::restore(&game_dir.join("options.txt"))?;
        fs::remove_file(marker)?;
    }
    Ok(())
}

pub fn prepare(store: &Store, instance: &Instance) -> Result<()> {
    let game_dir = store.game_dir(&instance.id);
    let enabled = instance.modules.performance && instance.modules.performance_options;
    prepare_options(&game_dir, enabled)
}

pub fn performance_core_count() -> Option<usize> {
    #[cfg(windows)]
    {
        windows_tuning::performance_core_mask().map(|(_, cores)| cores)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

pub fn tune_process(pid: u32, modules: &ModuleState, cores: Option<usize>) {
    #[cfg(windows)]
    {
        if modules.performance {
            windows_tuning::tune(
                pid,
                modules.performance_priority,
                modules.performance_ecoqos,
                modules.performance_affinity && cores.is_some(),
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, modules, cores);
    }
}

#[cfg(windows)]
mod windows_tuning {
    use std::ffi::c_void;

    type Handle = *mut c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn SetPriorityClass(handle: Handle, class: u32) -> i32;
        fn SetProcessAffinityMask(handle: Handle, mask: usize) -> i32;
        fn SetProcessInformation(handle: Handle, class: i32, info: *const c_void, size: u32) -> i32;
        fn GetLogicalProcessorInformationEx(
            relationship: u32,
            buffer: *mut c_void,
            length: *mut u32,
        ) -> i32;
    }

    const PROCESS_SET_INFORMATION: u32 = 0x0200;
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    const HIGH_PRIORITY_CLASS: u32 = 0x0000_0080;
    const RELATION_PROCESSOR_CORE: u32 = 0;
    const PROCESS_POWER_THROTTLING: i32 = 4;
    const POWER_THROTTLING_VERSION: u32 = 1;
    const POWER_THROTTLING_EXECUTION_SPEED: u32 = 0x1;

    #[repr(C)]
    struct PowerThrottlingState {
        version: u32,
        control_mask: u32,
        state_mask: u32,
    }

    pub fn performance_core_mask() -> Option<(usize, usize)> {
        let mut length: u32 = 0;
        unsafe {
            GetLogicalProcessorInformationEx(
                RELATION_PROCESSOR_CORE,
                std::ptr::null_mut(),
                &mut length,
            );
        }
        if length == 0 {
            return None;
        }
        let capacity = length as usize;
        let mut storage = vec![0u64; (capacity + 7) / 8];
        let ok = unsafe {
            GetLogicalProcessorInformationEx(
                RELATION_PROCESSOR_CORE,
                storage.as_mut_ptr() as *mut c_void,
                &mut length,
            )
        };
        if ok == 0 {
            return None;
        }
        let buffer = unsafe { std::slice::from_raw_parts(storage.as_ptr() as *const u8, capacity) };

        let mut offset = 0usize;
        let mut max_efficiency = 0u8;
        let mut cores: Vec<(u8, u16, usize)> = Vec::new();
        while offset + 8 <= length as usize {
            let size = u32::from_ne_bytes(buffer.get(offset + 4..offset + 8)?.try_into().ok()?)
                as usize;
            if size < 8 || offset + size > length as usize {
                break;
            }
            let efficiency = *buffer.get(offset + 9)?;
            let group_count =
                u16::from_ne_bytes(buffer.get(offset + 30..offset + 32)?.try_into().ok()?);
            let first_mask =
                u64::from_ne_bytes(buffer.get(offset + 32..offset + 40)?.try_into().ok()?)
                    as usize;
            max_efficiency = max_efficiency.max(efficiency);
            cores.push((efficiency, group_count, first_mask));
            offset += size;
        }

        let mut mask = 0usize;
        let mut core_count = 0usize;
        for (efficiency, group_count, first_mask) in cores {
            if efficiency == max_efficiency && group_count == 1 {
                mask |= first_mask;
                core_count += first_mask.count_ones() as usize;
            }
        }
        if mask == 0 {
            return None;
        }
        Some((mask, core_count.max(1)))
    }

    pub fn tune(pid: u32, priority: bool, ecoqos: bool, affinity: bool) {
        let handle = unsafe {
            OpenProcess(
                PROCESS_SET_INFORMATION | PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION,
                0,
                pid,
            )
        };
        if handle.is_null() {
            return;
        }
        if priority {
            unsafe {
                SetPriorityClass(handle, HIGH_PRIORITY_CLASS);
            }
        }
        if ecoqos {
            let state = PowerThrottlingState {
                version: POWER_THROTTLING_VERSION,
                control_mask: POWER_THROTTLING_EXECUTION_SPEED,
                state_mask: 0,
            };
            unsafe {
                SetProcessInformation(
                    handle,
                    PROCESS_POWER_THROTTLING,
                    &state as *const _ as *const c_void,
                    std::mem::size_of::<PowerThrottlingState>() as u32,
                );
            }
        }
        if affinity {
            if let Some((mask, _)) = performance_core_mask() {
                unsafe {
                    SetProcessAffinityMask(handle, mask);
                }
            }
        }
        unsafe {
            CloseHandle(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::apply_performance_jvm;

    #[test]
    fn adds_aikar_flags_and_aligns_memory() {
        let mut jvm = vec!["-Xms512M".to_string(), "-Xmx4096M".to_string()];
        apply_performance_jvm(&mut jvm, Some(8));
        assert!(jvm.iter().any(|argument| argument == "-XX:+UseG1GC"));
        assert!(jvm.iter().any(|argument| argument == "-Xms4096M"));
        assert!(jvm.iter().any(|argument| argument == "-XX:ActiveProcessorCount=8"));
        assert_eq!(
            jvm.iter()
                .filter(|argument| argument.as_str() == "-XX:+UseG1GC")
                .count(),
            1
        );

        apply_performance_jvm(&mut jvm, Some(8));
        assert_eq!(
            jvm.iter()
                .filter(|argument| argument.as_str() == "-XX:+UseG1GC")
                .count(),
            1
        );
        assert_eq!(
            jvm.iter()
                .filter(|argument| argument.as_str() == "-XX:ActiveProcessorCount=8")
                .count(),
            1
        );
    }
}
