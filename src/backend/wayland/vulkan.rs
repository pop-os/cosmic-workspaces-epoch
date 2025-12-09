use ash::{prelude::VkResult, vk};
use std::{collections::HashMap, ffi::CStr};

pub struct Vulkan {
    instance: ash::Instance,
    // TODO purge cache at some point
    device_name_cache: HashMap<u64, VkResult<Option<String>>>,
}

impl Vulkan {
    pub fn new() -> Option<Self> {
        let entry = unsafe { ash::Entry::load().ok()? };
        let app_info = vk::ApplicationInfo {
            api_version: vk::make_api_version(0, 1, 1, 0),
            ..Default::default()
        };
        let extensions = &[c"VK_EXT_physical_device_drm".as_ptr()];
        let create_info = vk::InstanceCreateInfo {
            p_application_info: &app_info,
            ..Default::default()
        }
        .enabled_extension_names(extensions);
        let instance = unsafe { entry.create_instance(&create_info, None).ok()? };
        Some(Self {
            instance,
            device_name_cache: HashMap::new(),
        })
    }

    pub fn device_name(&mut self, dev: u64) -> VkResult<Option<&str>> {
        if !self.device_name_cache.contains_key(&dev) {
            let value = self.device_name_uncached(dev);
            self.device_name_cache.insert(dev, value);
        }
        self.device_name_cache
            .get(&dev)
            .unwrap()
            .as_ref()
            .map(|x| x.as_deref())
            .map_err(|err| *err)
    }

    fn device_name_uncached(&mut self, dev: u64) -> VkResult<Option<String>> {
        let devices = unsafe { self.instance.enumerate_physical_devices()? };
        for device in devices {
            // Check extension is supported
            let supported = unsafe {
                self.instance
                    .enumerate_device_extension_properties(device)?
            };
            if !supported.iter().any(|ext| {
                CStr::from_bytes_until_nul(bytemuck::cast_slice(&ext.extension_name))
                    == Ok(ash::ext::physical_device_drm::NAME)
            }) {
                continue;
            }

            let mut drm_props = vk::PhysicalDeviceDrmPropertiesEXT::default();
            let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut drm_props);
            unsafe {
                self.instance
                    .get_physical_device_properties2(device, &mut props)
            };

            let device_name =
                CStr::from_bytes_until_nul(bytemuck::cast_slice(&props.properties.device_name));

            let major = rustix::fs::major(dev) as _;
            let minor = rustix::fs::minor(dev) as _;
            if (drm_props.primary_major, drm_props.primary_minor) == (major, minor)
                || (drm_props.render_major, drm_props.render_minor) == (major, minor)
            {
                return Ok(device_name
                    .ok()
                    .and_then(|x| Some(x.to_str().ok()?.to_owned())));
            }
        }

        Ok(None)
    }
}
