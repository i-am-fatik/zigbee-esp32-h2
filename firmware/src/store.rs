use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::ota_updater::OtaUpdater;
use esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN;
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use zigbee::{Credentials, Tables};

/// The data partition espflash lays down at 0x9000. Reflashing the application
/// leaves it alone, so a device keeps its network membership across updates.
/// The two records get a sector each, because they change at different times.
const CREDENTIALS: u32 = 0x9000;
const TABLES: u32 = 0xa000;
const SECTOR: u32 = 4096;
const WORD: u32 = 4;

pub struct Store {
    flash: FlashStorage<'static>,
    table: [u8; PARTITION_TABLE_MAX_LEN],
    sector: [u8; SECTOR as usize],
    filled: usize,
    written_to: u32,
    healthy: bool,
}

impl Store {
    pub fn new(flash: FLASH<'static>) -> Self {
        Self {
            flash: FlashStorage::new(flash),
            table: [0u8; PARTITION_TABLE_MAX_LEN],
            sector: [0xffu8; SECTOR as usize],
            filled: 0,
            written_to: 0,
            healthy: false,
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

    /// Starts an update by forgetting whatever a previous one left behind. The
    /// slot itself is erased a sector at a time as the image arrives.
    pub fn begin_firmware(&mut self) {
        self.filled = 0;
        self.written_to = 0;
        self.healthy = true;
    }

    /// Takes one piece of the image. Pieces arrive in order, so anything else
    /// means the download went wrong and the slot must not be booted.
    pub fn write_firmware(&mut self, offset: u32, data: &[u8]) -> bool {
        if !self.healthy || offset != self.written_to + self.filled as u32 {
            self.healthy = false;
            return false;
        }

        for byte in data {
            self.sector[self.filled] = *byte;
            self.filled += 1;
            if self.filled == SECTOR as usize && !self.flush() {
                return false;
            }
        }
        true
    }

    /// Writes the tail of the image and points the bootloader at the slot that
    /// now holds it, which is the moment the update becomes real.
    pub fn activate_firmware(&mut self) -> bool {
        if !self.healthy || (self.filled > 0 && !self.flush()) {
            return false;
        }
        let Self { flash, table, .. } = self;
        let Ok(mut updater) = OtaUpdater::new(flash, table) else {
            return false;
        };
        updater.activate_next_partition().is_ok()
            && updater.set_current_ota_state(OtaImageState::Valid).is_ok()
    }

    pub fn abandon_firmware(&mut self) {
        self.healthy = false;
        self.filled = 0;
    }

    fn flush(&mut self) -> bool {
        let at = self.written_to;
        let len = (self.filled as u32).next_multiple_of(WORD) as usize;
        self.sector[self.filled..len].fill(0xff);

        let Self {
            flash,
            table,
            sector,
            ..
        } = self;
        let Ok(mut updater) = OtaUpdater::new(flash, table) else {
            self.healthy = false;
            return false;
        };
        let Ok((mut slot, _)) = updater.next_partition() else {
            self.healthy = false;
            return false;
        };

        if slot.erase(at, at + SECTOR).is_err() || slot.write(at, &sector[..len]).is_err() {
            self.healthy = false;
            return false;
        }

        self.written_to += SECTOR;
        self.filled = 0;
        self.sector.fill(0xff);
        true
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
