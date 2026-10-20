// crates/nc-app/src/locale.rs
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Language {
    Turkish = 0,
    English = 1,
}

impl Language {
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Language::English,
            _ => Language::Turkish,
        }
    }

    pub const fn code(self) -> &'static str {
        match self {
            Language::Turkish => "tr",
            Language::English => "en",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum KeyboardLayout {
    TurkishQ = 0,
    UsQwerty = 1,
}

impl KeyboardLayout {
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => KeyboardLayout::UsQwerty,
            _ => KeyboardLayout::TurkishQ,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
pub enum TextId {
    SettingsTitle = 0,
    PageAppearance = 1,
    PageAppearanceSub = 2,
    PageEthernet = 3,
    PageEthernetSub = 4,
    PageWifi = 5,
    PageWifiSub = 6,
    PageBluetooth = 7,
    PageBluetoothSub = 8,
    PageDrivers = 9,
    PageDriversSub = 10,
    PageSystem = 11,
    PageSystemSub = 12,
    LiveWallpaperToggle = 13,
    SpeedSlow = 14,
    SpeedBalanced = 15,
    SpeedFluid = 16,
    AccentColorLabel = 17,
    BlurLabel = 18,
    BlurOff = 19,
    BlurLight = 20,
    BlurStandard = 21,
    BlurDeep = 22,
    DockAutoHideOn = 23,
    DockAutoHideOff = 24,
    EthRenewDhcp = 25,
    EthArpTest = 26,
    WifiScanning = 27,
    WifiScanBtn = 28,
    WifiRadioOff = 29,
    WifiConnectedBtn = 30,
    WifiConnectBtn = 31,
    Cancel = 32,
    BtSearching = 33,
    BtScanBtn = 34,
    BtRadioOff = 35,
    BtConnectedBtn = 36,
    DrvScanBtn = 37,
    DrvOptimizeBtn = 38,
    DrvCompileBtn = 39,
    DrvBuildInstallBtn = 40,
    LaunchSpeedLabel = 41,
    ModeCinematic = 42,
    ModeCinematicSub = 43,
    ModeBalanced = 44,
    ModeBalancedSub = 45,
    ModeInstant = 46,
    ModeInstantSub = 47,
    SystemLanguageLabel = 48,
    KeyboardLayoutLabel = 49,
    LangNameTr = 50,
    LangNameEn = 51,
    KbdNameTrQ = 52,
    KbdNameUs = 53,
    ResetDefaults = 54,
    StartButton = 55,
    CtxOpenTaskMgr = 56,
    CtxNewFolder = 57,
    CtxNewTextFile = 58,
    CtxRefreshIcons = 59,
    CtxNextWallpaper = 60,
    CtxLiveWpOn = 61,
    CtxLiveWpOff = 62,
    CtxOpenFiles = 63,
    CtxOpenSettings = 64,
    CtxOpenApp = 65,
    CtxUnpinDock = 66,
    CtxPinDock = 67,
    CtxUnpinStart = 68,
    CtxPinStart = 69,
    CtxCloseWin = 70,
    CtxReadyState = 71,
    CtxOpenEdit = 72,
    CtxRename = 73,
    CtxShowInFiles = 74,
    CtxDelete = 75,
    BtnSave = 76,
    BtnCancel = 77,
    BtnCreate = 78,
    BtnClose = 79,
    AppFiles = 80,
    AppStopwatch = 81,
    AppSystem = 82,
    AppBrowser = 83,
    LocDesktop = 84,
    LocDocuments = 85,
    LocDownloads = 86,
    LocMedia = 87,
    LocApps = 88,
    LocThisPc = 89,
    QuickAccess = 90,
    TagPersonalDisk = 91,
    TagSystemPart = 92,
    TagTempRam = 93,
    TagAllDrives = 94,
    BtnBack = 95,
    BtnPaste = 96,
    BtnNewFolder = 97,
    BtnNewDoc = 98,
    BadgePersonal = 99,
    BadgeSystem = 100,
    ColName = 101,
    ColType = 102,
    ColSize = 103,
    KindFolder = 104,
    KindPackage = 105,
    KindTextDoc = 106,
    KindImage = 107,
    KindVideo = 108,
    KindFile = 109,
    ActOpen = 110,
    ActCut = 111,
    ActCopy = 112,
    ActPasteHere = 113,
    ActNewFolder = 114,
}

const COUNT: usize = 115;
static TR_RAW: &str = include_str!("../langs/tr.lang");
static EN_RAW: &str = include_str!("../langs/en.lang");

pub const fn parse_lang_table(raw: &'static str) -> [&'static str; COUNT] {
    let bytes = raw.as_bytes();
    let mut table = [""; COUNT];
    let mut line_idx = 0usize;
    let mut start = 0usize;
    let mut i = 0usize;

    while i < bytes.len() && line_idx < COUNT {
        if bytes[i] == b'\n' {
            let mut end = i;
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
            if let Ok(s) = core::str::from_utf8( bytes.split_at(start).1.split_at(end - start).0 ) {
                table[line_idx] = s;
            }
            line_idx += 1;
            start = i + 1;
        }
        i += 1;
    }
    if start < bytes.len() && line_idx < COUNT {
        let mut end = bytes.len();
        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        if let Ok(s) = core::str::from_utf8( bytes.split_at(start).1.split_at(end - start).0 ) {
            table[line_idx] = s;
        }
    }
    table
}

static TR_TABLE: [&str; COUNT] = parse_lang_table(TR_RAW);
static EN_TABLE: [&str; COUNT] = parse_lang_table(EN_RAW);

pub fn tr(lang: Language, id: TextId) -> &'static str {
    let idx = id as usize;
    match lang {
        Language::Turkish => TR_TABLE[idx],
        Language::English => EN_TABLE[idx],
    }
}
