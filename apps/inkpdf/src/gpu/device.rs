//! Choosing the GPU: which backend, which adapter.

/// Which GPU to draw with. Showing pages needs almost no GPU power, so the default is the
/// integrated one, which is what keeps a laptop cool and its battery lasting.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum GpuChoice {
    /// The integrated GPU if there is one, otherwise the best other.
    #[default]
    Auto,
    Integrated,
    Discrete,
    /// The first GPU whose name contains this text, ignoring case.
    Named(String),
}

impl GpuChoice {
    pub fn parse(text: &str) -> GpuChoice {
        match text.to_ascii_lowercase().as_str() {
            "auto" => GpuChoice::Auto,
            "integrated" | "igpu" | "low" => GpuChoice::Integrated,
            "discrete" | "dgpu" | "high" => GpuChoice::Discrete,
            _ => GpuChoice::Named(text.to_string()),
        }
    }
}

fn backends() -> wgpu::Backends {
    // DX12 on Windows (its swap chain has the least latency); `WGPU_BACKEND` overrides it.
    wgpu::Backends::from_env().unwrap_or(wgpu::Backends::DX12)
}

pub(super) fn new_instance() -> wgpu::Instance {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = backends();
    wgpu::Instance::new(descriptor)
}

fn describe(info: &wgpu::AdapterInfo) -> String {
    format!("{} [{:?}, {:?}, driver {}]", info.name, info.device_type, info.backend, info.driver_info)
}

/// One line per GPU the app could use.
pub fn list_gpus() -> Vec<String> {
    let instance = new_instance();
    pollster::block_on(instance.enumerate_adapters(backends())).iter().map(|a| describe(&a.get_info())).collect()
}

pub(super) async fn pick_adapter(instance: &wgpu::Instance, surface: &wgpu::Surface<'_>, choice: &GpuChoice) -> Result<wgpu::Adapter, String> {
    let usable: Vec<wgpu::Adapter> = instance.enumerate_adapters(backends()).await.into_iter().filter(|a| a.is_surface_supported(surface)).collect();
    let find = |wanted: &dyn Fn(&wgpu::AdapterInfo) -> bool| usable.iter().find(|a| wanted(&a.get_info())).cloned();
    let available = || usable.iter().map(|a| describe(&a.get_info())).collect::<Vec<_>>().join("; ");
    let integrated = |i: &wgpu::AdapterInfo| i.device_type == wgpu::DeviceType::IntegratedGpu;
    match choice {
        GpuChoice::Integrated => find(&integrated).ok_or_else(|| format!("no integrated GPU can show this window; found: {}", available())),
        GpuChoice::Discrete => find(&|i| i.device_type == wgpu::DeviceType::DiscreteGpu)
            .ok_or_else(|| format!("no discrete GPU can show this window; found: {}", available())),
        GpuChoice::Named(name) => {
            let needle = name.to_ascii_lowercase();
            find(&|i| i.name.to_ascii_lowercase().contains(&needle)).ok_or_else(|| format!("no GPU named like \"{name}\"; found: {}", available()))
        }
        GpuChoice::Auto => match find(&integrated) {
            Some(a) => Ok(a),
            None => instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: Some(surface),
                    force_fallback_adapter: false,
                    apply_limit_buckets: false,
                })
                .await
                .map_err(|e| format!("no suitable GPU: {e}")),
        },
    }
}
