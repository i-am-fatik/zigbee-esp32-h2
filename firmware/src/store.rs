use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use zigbee::Credentials;

/// The data partition espflash lays down at 0x9000. Reflashing the application
/// leaves it alone, so a device keeps its network membership across updates.
const OFFSET: u32 = 0x9000;
const SECTOR: u32 = 4096;

pub struct Store {
    flash: FlashStorage<'static>,
}

impl Store {
    pub fn new(flash: FLASH<'static>) -> Self {
        Self {
            flash: FlashStorage::new(flash),
        }
    }

    pub fn load(&mut self) -> Option<Credentials> {
        let mut record = [0u8; Credentials::SIZE];
        self.flash.read(OFFSET, &mut record).ok()?;
        Credentials::from_bytes(&record)
    }

    pub fn save(&mut self, credentials: &Credentials) -> bool {
        if self.flash.erase(OFFSET, OFFSET + SECTOR).is_err() {
            return false;
        }
        self.flash.write(OFFSET, &credentials.to_bytes()).is_ok()
    }

    pub fn forget(&mut self) {
        let _ = self.flash.erase(OFFSET, OFFSET + SECTOR);
    }
}
