use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use zigbee::{Credentials, Tables};

/// The data partition espflash lays down at 0x9000. Reflashing the application
/// leaves it alone, so a device keeps its network membership across updates.
/// The two records get a sector each, because they change at different times.
const CREDENTIALS: u32 = 0x9000;
const TABLES: u32 = 0xa000;
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
        Credentials::from_bytes(&self.read(CREDENTIALS)?)
    }

    pub fn save(&mut self, credentials: &Credentials) -> bool {
        self.write(CREDENTIALS, &credentials.to_bytes())
    }

    pub fn load_tables(&mut self) -> Option<Tables> {
        Tables::from_bytes(&self.read(TABLES)?)
    }

    pub fn save_tables(&mut self, tables: &Tables) -> bool {
        self.write(TABLES, &tables.to_bytes())
    }

    /// Leaving a network makes both records meaningless, because the groups and
    /// the scenes belonged to the coordinator that is being left behind.
    pub fn forget(&mut self) {
        let _ = self.flash.erase(CREDENTIALS, CREDENTIALS + SECTOR);
        let _ = self.flash.erase(TABLES, TABLES + SECTOR);
    }

    fn read<const N: usize>(&mut self, offset: u32) -> Option<[u8; N]> {
        let mut record = [0u8; N];
        self.flash.read(offset, &mut record).ok()?;
        Some(record)
    }

    fn write(&mut self, offset: u32, record: &[u8]) -> bool {
        if self.flash.erase(offset, offset + SECTOR).is_err() {
            return false;
        }
        self.flash.write(offset, record).is_ok()
    }
}
