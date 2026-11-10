// Otomatik uretildi: tools/gen_r3_strings.py (elle duzenlemeyin).
// Sutunlar: Running, Stopped, Start, Stop, Reset. Satir sirasi nc_i18n::Language degeri.
pub const RUNNING: usize = 0;
pub const STOPPED: usize = 1;
pub const START: usize = 2;
pub const STOP: usize = 3;
pub const RESET: usize = 4;
pub const COLUMNS: usize = 5;
pub const LANGUAGES: usize = 11;
pub const TABLE: [[&str; COLUMNS]; LANGUAGES] = [
    // tr
    ["Çalışıyor", "Durdu", "Başlat", "Durdur", "Sıfırla"],
    // en
    ["Running", "Stopped", "Start", "Stop", "Reset"],
    // fr
    ["En marche", "Arrêté", "Démarrer", "Arrêter", "Réinitialiser"],
    // de
    ["Läuft", "Gestoppt", "Start", "Stopp", "Zurücksetzen"],
    // es
    ["En marcha", "Detenido", "Iniciar", "Detener", "Restablecer"],
    // pt
    ["Em andamento", "Parado", "Iniciar", "Parar", "Zerar"],
    // ru
    ["Идёт", "Остановлен", "Старт", "Стоп", "Сброс"],
    // zh
    ["运行中", "已停止", "开始", "停止", "重置"],
    // ja
    ["計測中", "停止中", "開始", "停止", "リセット"],
    // ko
    ["작동 중", "정지됨", "시작", "정지", "초기화"],
    // hi
    ["चल रहा है", "रुका हुआ", "शुरू करें", "रोकें", "रीसेट करें"],
];

/// Bilinmeyen dil kimliği İngilizce'ye (1) düşer.
pub fn for_language(id: u32) -> &'static [&'static str; COLUMNS] {
    let index = if (id as usize) < LANGUAGES { id as usize } else { 1 };
    &TABLE[index]
}

