//! Mehrsprachigkeit: Deutsch, Englisch, Französisch, Russisch, Spanisch, Chinesisch (vereinfacht).
//!
//! Alle Texte stehen in einer Tabelle `Schlüssel → [de, en, fr, ru, es, zh]`.
//! Die aktive Sprache ist global (gilt für Oberfläche und Engine-Thread gleichermaßen).
//! Platzhalter `{0}`, `{1}` … werden mit `tf` ersetzt.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// Unterstützte Sprachen (Reihenfolge = Spalten der Texttabelle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Lang {
    De,
    #[default]
    En,
    Fr,
    Ru,
    Es,
    Zh,
}

impl Lang {
    /// Alle Sprachen in Anzeigereihenfolge.
    pub const ALL: [Lang; 6] = [Lang::De, Lang::En, Lang::Fr, Lang::Ru, Lang::Es, Lang::Zh];

    /// Name der Sprache in ihr selbst (für die Auswahlliste).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::De => "Deutsch",
            Lang::En => "English",
            Lang::Fr => "Français",
            Lang::Ru => "Русский",
            Lang::Es => "Español",
            Lang::Zh => "简体中文",
        }
    }

    /// Spalte in der Texttabelle.
    fn index(self) -> usize {
        self as usize
    }

    /// Sprache aus einer Windows-Sprachkennung (LANGID); Unbekanntes wird Englisch.
    pub fn from_windows_langid(langid: u16) -> Lang {
        match langid & 0x3FF {
            0x07 => Lang::De,
            0x0C => Lang::Fr,
            0x19 => Lang::Ru,
            0x0A => Lang::Es,
            0x04 => Lang::Zh,
            _ => Lang::En,
        }
    }

    /// Sprache aus einer POSIX-Locale wie `de_DE.UTF-8` oder `zh_CN`; Unbekanntes wird Englisch.
    pub fn from_locale(locale: &str) -> Lang {
        match locale.get(..2).map(str::to_ascii_lowercase).as_deref() {
            Some("de") => Lang::De,
            Some("fr") => Lang::Fr,
            Some("ru") => Lang::Ru,
            Some("es") => Lang::Es,
            Some("zh") => Lang::Zh,
            _ => Lang::En,
        }
    }

    /// Wandelt den gespeicherten Index zurück in eine Sprache.
    fn from_index(i: u8) -> Lang {
        Lang::ALL.get(i as usize).copied().unwrap_or(Lang::En)
    }
}

/// Aktive Sprache (global, threadübergreifend).
static CURRENT: AtomicU8 = AtomicU8::new(Lang::En as u8);

/// Setzt die aktive Sprache.
pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

/// Liefert die aktive Sprache.
pub fn lang() -> Lang {
    Lang::from_index(CURRENT.load(Ordering::Relaxed))
}

/// Übersetzt einen Schlüssel in die aktive Sprache (Rückfall: Englisch, dann der Schlüssel selbst).
pub fn t(key: &str) -> &'static str {
    match table().get(key) {
        Some(texts) => {
            let s = texts[lang().index()];
            if s.is_empty() { texts[Lang::En.index()] } else { s }
        }
        None => Box::leak(key.to_string().into_boxed_str()),
    }
}

/// Übersetzt und ersetzt die Platzhalter `{0}`, `{1}` … durch die Argumente.
pub fn tf(key: &str, args: &[&dyn Display]) -> String {
    let mut s = t(key).to_string();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), &a.to_string());
    }
    s
}

/// Nachschlage-Tabelle, beim ersten Zugriff aus `TEXTS` aufgebaut.
fn table() -> &'static HashMap<&'static str, [&'static str; 6]> {
    static TABLE: OnceLock<HashMap<&'static str, [&'static str; 6]>> = OnceLock::new();
    TABLE.get_or_init(|| TEXTS.iter().map(|(k, v)| (*k, *v)).collect())
}

/// Alle Texte: Schlüssel → [Deutsch, Englisch, Französisch, Russisch, Spanisch, Chinesisch].
#[rustfmt::skip]
const TEXTS: &[(&str, [&str; 6])] = &[
    // ---------------------------------------------------------------- Allgemein / Kopfzeile
    ("tab.dashboard", ["Dashboard", "Dashboard", "Tableau de bord", "Панель", "Panel", "仪表板"]),
    ("tab.layouts", ["Layouts", "Layouts", "Dispositions", "Раскладки", "Diseños", "布局"]),
    ("tab.preview", ["Vorschau", "Previews", "Aperçus", "Превью", "Vistas previas", "预览"]),
    ("tab.performance", ["Leistung", "Performance", "Performances", "Производительность", "Rendimiento", "性能"]),
    ("tab.hotkeys", ["Hotkeys", "Hotkeys", "Raccourcis", "Горячие клавиши", "Atajos", "快捷键"]),
    ("tab.settings", ["Einstellungen", "Settings", "Paramètres", "Настройки", "Ajustes", "设置"]),
    ("top.profile", ["Profil:", "Profile:", "Profil :", "Профиль:", "Perfil:", "配置:"]),
    ("top.apply_layout", ["Layout anwenden", "Apply layout", "Appliquer la disposition", "Применить раскладку", "Aplicar diseño", "应用布局"]),
    ("top.previews_on", ["Vorschau: an", "Previews: on", "Aperçus : activés", "Превью: вкл", "Vistas: activadas", "预览:开"]),
    ("top.previews_off", ["Vorschau: aus", "Previews: off", "Aperçus : désactivés", "Превью: выкл", "Vistas: desactivadas", "预览:关"]),
    ("status.saving", ["speichert …", "saving …", "enregistrement …", "сохранение …", "guardando …", "正在保存 …"]),
    ("status.saved", ["gespeichert", "saved", "enregistré", "сохранено", "guardado", "已保存"]),
    ("status.save_failed", ["Speichern fehlgeschlagen: {0}", "Saving failed: {0}", "Échec de l'enregistrement : {0}", "Ошибка сохранения: {0}", "Error al guardar: {0}", "保存失败:{0}"]),
    ("log.config", ["Konfiguration: {0}", "Configuration: {0}", "Configuration : {0}", "Конфигурация: {0}", "Configuración: {0}", "配置文件:{0}"]),
    ("log.layout_captured", ["Layout erfasst: Hauptfenster + {0} Nebenfenster", "Layout captured: main window + {0} secondary windows", "Disposition capturée : fenêtre principale + {0} fenêtres secondaires", "Раскладка сохранена: главное окно + {0} дополнительных", "Diseño capturado: ventana principal + {0} secundarias", "已捕获布局:主窗口 + {0} 个副窗口"]),
    ("watermark", ["EVE Multibox · © 2026 LeggaSchnegge", "EVE Multibox · © 2026 LeggaSchnegge", "EVE Multibox · © 2026 LeggaSchnegge", "EVE Multibox · © 2026 LeggaSchnegge", "EVE Multibox · © 2026 LeggaSchnegge", "EVE Multibox · © 2026 LeggaSchnegge"]),

    // ---------------------------------------------------------------- Dashboard
    ("dash.warn_hotkeys", ["{0} Hotkey-Problem(e) – siehe Tab „Hotkeys“", "{0} hotkey problem(s) – see the “Hotkeys” tab", "{0} problème(s) de raccourci – voir l'onglet « Raccourcis »", "Проблем с горячими клавишами: {0} – см. вкладку «Горячие клавиши»", "{0} problema(s) de atajos – ver la pestaña «Atajos»", "{0} 个快捷键问题 – 请查看“快捷键”页"]),
    ("dash.trim_bg", ["RAM der Hintergrund-Clients freigeben", "Free RAM of background clients", "Libérer la RAM des clients en arrière-plan", "Освободить ОЗУ фоновых клиентов", "Liberar RAM de los clientes en segundo plano", "释放后台客户端内存"]),
    ("dash.trim_bg_hint", ["Leert den Arbeitsspeicher aller Clients außer dem aktiven. Siehe Tab „Leistung“.", "Empties the working set of all clients except the active one. See the “Performance” tab.", "Vide la mémoire de travail de tous les clients sauf l'actif. Voir l'onglet « Performances ».", "Очищает рабочий набор всех клиентов, кроме активного. См. вкладку «Производительность».", "Vacía la memoria de trabajo de todos los clientes excepto el activo. Ver la pestaña «Rendimiento».", "清空除当前客户端外所有客户端的工作集。参见“性能”页。"]),
    ("dash.no_clients", ["Keine EVE-Clients gefunden.", "No EVE clients found.", "Aucun client EVE trouvé.", "Клиенты EVE не найдены.", "No se encontraron clientes de EVE.", "未找到 EVE 客户端。"]),
    ("dash.no_clients_hint", ["Starte EVE im Fenstermodus („Fenster“ oder „Fenster (fest)“). Clients werden automatisch erkannt.", "Start EVE in windowed mode (“Window” or “Fixed window”). Clients are detected automatically.", "Lancez EVE en mode fenêtré (« Fenêtre » ou « Fenêtre fixe »). Les clients sont détectés automatiquement.", "Запустите EVE в оконном режиме («Окно» или «Фиксированное окно»). Клиенты определяются автоматически.", "Inicia EVE en modo ventana («Ventana» o «Ventana fija»). Los clientes se detectan automáticamente.", "请以窗口模式(“窗口”或“固定窗口”)启动 EVE,客户端会被自动识别。"]),
    ("dash.sys_cpu", ["System-CPU", "System CPU", "CPU système", "ЦП системы", "CPU del sistema", "系统 CPU"]),
    ("dash.cores", ["{0} logische Kerne", "{0} logical cores", "{0} cœurs logiques", "Логических ядер: {0}", "{0} núcleos lógicos", "{0} 个逻辑核心"]),
    ("dash.sys_ram", ["System-RAM", "System RAM", "RAM système", "ОЗУ системы", "RAM del sistema", "系统内存"]),
    ("dash.free", ["{0} frei", "{0} free", "{0} libre", "свободно {0}", "{0} libre", "可用 {0}"]),
    ("dash.eve_total", ["EVE gesamt", "EVE total", "Total EVE", "Всего EVE", "EVE total", "EVE 合计"]),
    ("dash.working_set", ["{0} Arbeitsspeicher", "{0} working set", "{0} de mémoire de travail", "рабочий набор {0}", "{0} de memoria de trabajo", "工作集 {0}"]),
    ("dash.clients", ["Clients", "Clients", "Clients", "Клиенты", "Clientes", "客户端"]),
    ("dash.logged_in", ["{0} eingeloggt", "{0} logged in", "{0} connecté(s)", "в игре: {0}", "{0} conectados", "{0} 个已登录"]),
    ("dash.spark_hint", ["Spitze {0} %, Skala bis {1} %", "Peak {0} %, scale up to {1} %", "Pic {0} %, échelle jusqu'à {1} %", "Пик {0} %, шкала до {1} %", "Pico {0} %, escala hasta {1} %", "峰值 {0} %,刻度至 {1} %"]),
    ("dash.main_marker", ["Hauptfenster", "Main window", "Fenêtre principale", "Главное окно", "Ventana principal", "主窗口"]),
    ("col.character", ["Charakter", "Character", "Personnage", "Персонаж", "Personaje", "角色"]),
    ("col.status", ["Status", "Status", "Statut", "Статус", "Estado", "状态"]),
    ("col.cpu", ["CPU", "CPU", "CPU", "ЦП", "CPU", "CPU"]),
    ("col.history", ["Verlauf (60 s)", "History (60 s)", "Historique (60 s)", "История (60 с)", "Historial (60 s)", "历史 (60 秒)"]),
    ("col.ram", ["RAM", "RAM", "RAM", "ОЗУ", "RAM", "内存"]),
    ("col.private", ["Privat", "Private", "Privée", "Частная", "Privada", "私有"]),
    ("col.priority", ["Priorität", "Priority", "Priorité", "Приоритет", "Prioridad", "优先级"]),
    ("col.cores", ["Kerne", "Cores", "Cœurs", "Ядра", "Núcleos", "核心"]),
    ("col.hotkey", ["Hotkey", "Hotkey", "Raccourci", "Сочетание", "Atajo", "快捷键"]),
    ("col.prio_active", ["Priorität (aktiv)", "Priority (active)", "Priorité (actif)", "Приоритет (активен)", "Prioridad (activo)", "优先级(活动时)"]),
    ("btn.focus", ["Fokus", "Focus", "Focus", "Фокус", "Enfocar", "聚焦"]),
    ("btn.main", ["Haupt", "Main", "Principal", "Главное", "Principal", "主"]),
    ("btn.main_hint", ["Ins Hauptfenster tauschen", "Swap into main window", "Placer dans la fenêtre principale", "Переместить в главное окно", "Mover a la ventana principal", "切换到主窗口"]),
    ("btn.trim", ["RAM frei", "Free RAM", "Libérer RAM", "Освободить", "Liberar RAM", "释放内存"]),
    ("btn.trim_hint", ["Arbeitsspeicher dieses Clients freigeben", "Free this client's working set", "Libérer la mémoire de travail de ce client", "Освободить рабочий набор этого клиента", "Liberar la memoria de trabajo de este cliente", "释放此客户端的工作集"]),
    ("status.no_rights", ["Keine Rechte", "No rights", "Pas de droits", "Нет прав", "Sin permisos", "无权限"]),
    ("status.login", ["Login", "Login", "Connexion", "Вход", "Inicio de sesión", "登录界面"]),
    ("status.active", ["Aktiv", "Active", "Actif", "Активен", "Activo", "活动"]),
    ("status.minimized", ["Minimiert", "Minimized", "Réduit", "Свёрнут", "Minimizado", "已最小化"]),
    ("status.throttled", ["Gedrosselt", "Throttled", "Bridé", "Ограничен", "Limitado", "已限速"]),
    ("status.background", ["Hintergrund", "Background", "Arrière-plan", "Фон", "Segundo plano", "后台"]),

    // ---------------------------------------------------------------- Engine-Meldungen
    ("eng.warn_ram", ["{0}: {1} GB Arbeitsspeicher (Grenze {2} GB)", "{0}: {1} GB working set (limit {2} GB)", "{0} : {1} Go de mémoire (limite {2} Go)", "{0}: {1} ГБ ОЗУ (предел {2} ГБ)", "{0}: {1} GB de memoria (límite {2} GB)", "{0}:工作集 {1} GB(上限 {2} GB)"]),
    ("eng.warn_rights", ["{0}: Prozess-Einstellungen nicht möglich – läuft EVE mit Adminrechten?", "{0}: cannot change process settings – is EVE running as administrator?", "{0} : impossible de modifier le processus – EVE tourne-t-il en administrateur ?", "{0}: невозможно изменить параметры процесса – EVE запущен от администратора?", "{0}: no se pueden cambiar los ajustes del proceso – ¿EVE se ejecuta como administrador?", "{0}:无法修改进程设置 – EVE 是否以管理员身份运行?"]),
    ("eng.warn_sysram", ["System: nur noch {0} GB von {1} GB RAM frei", "System: only {0} GB of {1} GB RAM free", "Système : seulement {0} Go libres sur {1} Go", "Система: свободно лишь {0} ГБ из {1} ГБ ОЗУ", "Sistema: solo {0} GB libres de {1} GB de RAM", "系统:{1} GB 内存中仅剩 {0} GB 可用"]),
    ("eng.focus_denied", ["Windows hat den Fokuswechsel verweigert – bitte erneut versuchen.", "Windows refused the focus change – please try again.", "Windows a refusé le changement de focus – réessayez.", "Windows отказал в смене фокуса – попробуйте снова.", "Windows rechazó el cambio de foco – inténtalo de nuevo.", "Windows 拒绝了焦点切换 – 请重试。"]),
    ("eng.trimmed", ["{0} Client(s): {1} MB Arbeitsspeicher freigegeben", "{0} client(s): {1} MB of working set freed", "{0} client(s) : {1} Mo de mémoire libérés", "Клиентов: {0}, освобождено {1} МБ", "{0} cliente(s): {1} MB de memoria liberados", "{0} 个客户端:已释放 {1} MB 工作集"]),
    ("eng.no_clients_capture", ["Keine EVE-Clients gefunden – nichts zu erfassen.", "No EVE clients found – nothing to capture.", "Aucun client EVE – rien à capturer.", "Клиенты EVE не найдены – нечего сохранять.", "No hay clientes de EVE – nada que capturar.", "未找到 EVE 客户端 – 无可捕获内容。"]),
    ("eng.hk_dup", ["{0}: {1} ist schon für „{2}“ vergeben", "{0}: {1} is already used for “{2}”", "{0} : {1} est déjà utilisé pour « {2} »", "{0}: {1} уже занят для «{2}»", "{0}: {1} ya está asignado a «{2}»", "{0}:{1} 已分配给“{2}”"]),
    ("eng.hk_taken", ["{0}: {1} ist von einem anderen Programm belegt", "{0}: {1} is taken by another program", "{0} : {1} est utilisé par un autre programme", "{0}: {1} занят другой программой", "{0}: {1} está ocupado por otro programa", "{0}:{1} 已被其他程序占用"]),
    ("preview.minimized", ["(minimiert)", "(minimized)", "(réduit)", "(свёрнут)", "(minimizado)", "(已最小化)"]),

    // ---------------------------------------------------------------- Hotkey-Aktionen & Parser
    ("hk.next", ["Nächster Client", "Next client", "Client suivant", "Следующий клиент", "Siguiente cliente", "下一个客户端"]),
    ("hk.prev", ["Vorheriger Client", "Previous client", "Client précédent", "Предыдущий клиент", "Cliente anterior", "上一个客户端"]),
    ("hk.swap", ["Hauptfenster tauschen", "Swap main window", "Échanger la fenêtre principale", "Сменить главное окно", "Cambiar ventana principal", "切换主窗口"]),
    ("hk.toggle", ["Vorschau ein/aus", "Toggle previews", "Afficher/masquer les aperçus", "Вкл/выкл превью", "Mostrar/ocultar vistas", "显示/隐藏预览"]),
    ("hkp.empty_part", ["Leerer Teil im Hotkey (doppeltes „+“?)", "Empty part in hotkey (double “+”?)", "Partie vide dans le raccourci (« + » en double ?)", "Пустая часть в сочетании (двойной «+»?)", "Parte vacía en el atajo (¿«+» doble?)", "快捷键中有空项(重复的“+”?)"]),
    ("hkp.multi_key", ["Mehr als eine Taste im Hotkey: „{0}“", "More than one key in hotkey: “{0}”", "Plus d'une touche dans le raccourci : « {0} »", "Больше одной клавиши в сочетании: «{0}»", "Más de una tecla en el atajo: «{0}»", "快捷键中包含多个按键:“{0}”"]),
    ("hkp.unknown", ["Unbekannte Taste: „{0}“", "Unknown key: “{0}”", "Touche inconnue : « {0} »", "Неизвестная клавиша: «{0}»", "Tecla desconocida: «{0}»", "未知按键:“{0}”"]),
    ("hkp.no_key", ["Hotkey braucht eine Taste, nicht nur Modifier", "A hotkey needs a key, not just modifiers", "Un raccourci nécessite une touche, pas seulement des modificateurs", "Нужна клавиша, а не только модификаторы", "El atajo necesita una tecla, no solo modificadores", "快捷键需要一个按键,不能只有修饰键"]),
    ("hk.field_hint", ["z. B. Ctrl+F1", "e.g. Ctrl+F1", "ex. Ctrl+F1", "напр. Ctrl+F1", "p. ej. Ctrl+F1", "例如 Ctrl+F1"]),
    ("hk.recognized", ["Erkannt als {0}", "Recognized as {0}", "Reconnu comme {0}", "Распознано как {0}", "Reconocido como {0}", "识别为 {0}"]),
    ("hk.empty", ["Leer = kein Hotkey", "Empty = no hotkey", "Vide = aucun raccourci", "Пусто = без сочетания", "Vacío = sin atajo", "留空 = 无快捷键"]),
    ("hk.record", ["Aufnehmen", "Record", "Enregistrer", "Записать", "Grabar", "录制"]),
    ("hk.recording", ["Tasten drücken … (Esc = Abbruch, Entf = löschen)", "Press keys … (Esc = cancel, Del = clear)", "Appuyez … (Échap = annuler, Suppr = effacer)", "Нажмите клавиши … (Esc – отмена, Del – очистить)", "Pulsa teclas … (Esc = cancelar, Supr = borrar)", "请按键 …(Esc 取消,Del 清除)"]),
    ("hk.record_hint", ["Klicken und dann die gewünschte Tastenkombination drücken", "Click, then press the desired key combination", "Cliquez puis appuyez sur la combinaison souhaitée", "Нажмите и затем введите нужное сочетание", "Haz clic y pulsa la combinación deseada", "点击后按下想要的组合键"]),

    // ---------------------------------------------------------------- Prioritäten & Kerne
    ("prio.idle", ["Niedrig", "Low", "Basse", "Низкий", "Baja", "低"]),
    ("prio.below", ["Niedriger als normal", "Below normal", "Inférieure à la normale", "Ниже среднего", "Por debajo de lo normal", "低于正常"]),
    ("prio.normal", ["Normal", "Normal", "Normale", "Обычный", "Normal", "正常"]),
    ("prio.above", ["Höher als normal", "Above normal", "Supérieure à la normale", "Выше среднего", "Por encima de lo normal", "高于正常"]),
    ("prio.high", ["Hoch", "High", "Haute", "Высокий", "Alta", "高"]),
    ("prio.default", ["Standard", "Default", "Par défaut", "По умолчанию", "Predeterminado", "默认"]),
    ("cores.all", ["alle", "all", "tous", "все", "todos", "全部"]),
    ("cores.even", ["gerade", "even", "pairs", "чётные", "pares", "偶数"]),
    ("cores.even_hint", ["Nur physische Kerne bei SMT/Hyper-Threading (0, 2, 4 …)", "Physical cores only with SMT/Hyper-Threading (0, 2, 4 …)", "Seulement les cœurs physiques avec SMT/Hyper-Threading (0, 2, 4 …)", "Только физические ядра при SMT/Hyper-Threading (0, 2, 4 …)", "Solo núcleos físicos con SMT/Hyper-Threading (0, 2, 4 …)", "启用超线程时仅使用物理核心(0、2、4 …)"]),
    ("cores.first_half", ["1. Hälfte", "1st half", "1re moitié", "1-я половина", "1.ª mitad", "前半"]),
    ("cores.second_half", ["2. Hälfte", "2nd half", "2e moitié", "2-я половина", "2.ª mitad", "后半"]),
    ("rect.w", ["B", "W", "L", "Ш", "An", "宽"]),
    ("rect.h", ["H", "H", "H", "В", "Al", "高"]),

    // ---------------------------------------------------------------- Layouts
    ("lay.section_options", ["Optionen", "Options", "Options", "Параметры", "Opciones", "选项"]),
    ("lay.borderless", ["Rahmenlos (Titelleiste und Rand entfernen)", "Borderless (remove title bar and frame)", "Sans bordure (supprimer barre de titre et cadre)", "Без рамки (убрать заголовок и рамку)", "Sin bordes (quitar barra de título y marco)", "无边框(移除标题栏和边框)"]),
    ("lay.borderless_hint", ["Alternativ in EVE den Anzeigemodus „Fenster (fest)“ wählen.", "Alternatively choose the “Fixed window” display mode in EVE.", "Ou choisissez le mode « Fenêtre fixe » dans EVE.", "Либо выберите в EVE режим «Фиксированное окно».", "O elige el modo «Ventana fija» en EVE.", "也可以在 EVE 中选择“固定窗口”显示模式。"]),
    ("lay.auto_apply", ["Automatisch anwenden, wenn ein Client startet oder sich ein Charakter einloggt", "Apply automatically when a client starts or a character logs in", "Appliquer automatiquement au lancement d'un client ou à la connexion d'un personnage", "Применять автоматически при запуске клиента или входе персонажа", "Aplicar automáticamente al iniciar un cliente o conectar un personaje", "客户端启动或角色登录时自动应用"]),
    ("lay.swap_on_activate", ["Beim Umschalten (Hotkey/Vorschau) den Client ins Hauptfenster tauschen", "When switching (hotkey/preview), swap the client into the main window", "Lors du changement (raccourci/aperçu), placer le client dans la fenêtre principale", "При переключении (клавиша/превью) перемещать клиента в главное окно", "Al cambiar (atajo/vista), mover el cliente a la ventana principal", "切换时(快捷键/预览)将客户端换到主窗口"]),
    ("lay.default_main", ["Hauptfenster beim Start:", "Main window on start:", "Fenêtre principale au démarrage :", "Главное окно при запуске:", "Ventana principal al inicio:", "启动时的主窗口:"]),
    ("lay.character_name", ["Charaktername", "Character name", "Nom du personnage", "Имя персонажа", "Nombre del personaje", "角色名"]),
    ("lay.fg_or_first", ["(Vordergrund / erster)", "(foreground / first)", "(premier plan / premier)", "(активный / первый)", "(primer plano / primero)", "(前台 / 第一个)"]),
    ("lay.section_overview", ["Übersicht", "Overview", "Vue d'ensemble", "Обзор", "Vista general", "概览"]),
    ("lay.capture", ["Aktuelle Fensterpositionen übernehmen", "Capture current window positions", "Capturer les positions actuelles", "Сохранить текущие позиции окон", "Capturar posiciones actuales", "采用当前窗口位置"]),
    ("lay.capture_hint", ["Ordne die Clients von Hand an und klicke dann hier: Hauptfenster und Nebenfenster werden ins Profil übernommen.", "Arrange the clients by hand, then click here: main and secondary windows are saved to the profile.", "Placez les clients à la main puis cliquez ici : les fenêtres sont enregistrées dans le profil.", "Расставьте клиентов вручную и нажмите здесь: окна сохранятся в профиль.", "Coloca los clientes a mano y haz clic aquí: las ventanas se guardan en el perfil.", "先手动摆放客户端,再点击此处:主窗口和副窗口将保存到配置中。"]),
    ("lay.apply_now", ["Layout jetzt anwenden", "Apply layout now", "Appliquer maintenant", "Применить сейчас", "Aplicar ahora", "立即应用布局"]),
    ("lay.section_slots", ["Plätze", "Slots", "Emplacements", "Слоты", "Posiciones", "窗口位"]),
    ("lay.secondary_n", ["Nebenfenster {0}", "Secondary {0}", "Secondaire {0}", "Дополнительное {0}", "Secundaria {0}", "副窗口 {0}"]),
    ("lay.add_slot", ["+ Nebenfenster-Platz", "+ Secondary slot", "+ Emplacement secondaire", "+ Доп. слот", "+ Posición secundaria", "+ 副窗口位"]),
    ("lay.section_presets", ["Monitor-Vorlagen", "Monitor presets", "Modèles d'écran", "Шаблоны мониторов", "Plantillas de monitor", "显示器模板"]),
    ("lay.monitors_found", ["{0} Monitor(e) erkannt", "{0} monitor(s) detected", "{0} écran(s) détecté(s)", "Обнаружено мониторов: {0}", "{0} monitor(es) detectado(s)", "检测到 {0} 台显示器"]),
    ("lay.rescan", ["Neu einlesen", "Rescan", "Actualiser", "Обновить", "Volver a detectar", "重新检测"]),
    ("lay.preset_stacked", ["Alle gleich groß (Vollbild, gestapelt)", "All same size (fullscreen, stacked)", "Tous identiques (plein écran, empilés)", "Все одинаковые (полный экран, стопкой)", "Todos iguales (pantalla completa, apilados)", "全部同尺寸(全屏叠放)"]),
    ("lay.preset_grid", ["Haupt + Raster", "Main + grid", "Principal + grille", "Главное + сетка", "Principal + cuadrícula", "主窗口 + 网格"]),
    ("lay.preset_sidebar", ["Haupt + Seitenleiste", "Main + sidebar", "Principal + barre latérale", "Главное + боковая панель", "Principal + barra lateral", "主窗口 + 侧栏"]),
    ("lay.exclude_taskbar", ["Taskleiste aussparen", "Leave taskbar free", "Laisser la barre des tâches libre", "Не перекрывать панель задач", "Dejar libre la barra de tareas", "避开任务栏"]),
    ("lay.main_on", ["Hauptfenster auf:", "Main window on:", "Fenêtre principale sur :", "Главное окно на:", "Ventana principal en:", "主窗口所在:"]),
    ("lay.clients_max", ["Clients (max.)", "Clients (max.)", "Clients (max.)", "Клиентов (макс.)", "Clientes (máx.)", "客户端(最多)"]),
    ("lay.clients_max_hint", ["Lieber etwas mehr angeben – unbenutzte Plätze schaden nicht.", "Better too many – unused slots don't hurt.", "Prévoyez large – les emplacements inutilisés ne gênent pas.", "Лучше с запасом – лишние слоты не мешают.", "Mejor de sobra – las posiciones sin usar no molestan.", "可以多填一些 – 未使用的窗口位没有影响。"]),
    ("lay.grid_on", ["Raster auf:", "Grid on:", "Grille sur :", "Сетка на:", "Cuadrícula en:", "网格所在:"]),
    ("lay.cols", ["Spalten", "Columns", "Colonnes", "Столбцы", "Columnas", "列"]),
    ("lay.rows", ["Zeilen", "Rows", "Lignes", "Строки", "Filas", "行"]),
    ("lay.secondaries", ["Nebenfenster", "Secondary windows", "Fenêtres secondaires", "Доп. окна", "Ventanas secundarias", "副窗口"]),
    ("lay.width_pct", ["Breite %", "Width %", "Largeur %", "Ширина %", "Ancho %", "宽度 %"]),
    ("lay.sidebar_left", ["Leiste links", "Sidebar left", "Barre à gauche", "Панель слева", "Barra a la izquierda", "侧栏在左"]),
    ("lay.stacked_hint", ["Jeder Client bekommt den ganzen Monitor. Vorschau anklicken oder Hotkey drücken holt ihn nach vorne. Schaltet auch „Rahmenlos“ ein – alternativ in EVE „Fenster (fest)“ wählen.", "Each client gets the whole monitor. Click its preview or press a hotkey to bring it to the front. Also enables “Borderless” – or choose “Fixed window” in EVE.", "Chaque client occupe tout l'écran. Cliquez sur son aperçu ou utilisez un raccourci pour l'afficher. Active aussi « Sans bordure » – ou choisissez « Fenêtre fixe » dans EVE.", "Каждый клиент занимает весь монитор. Клик по превью или горячая клавиша выводит его вперёд. Также включает «Без рамки» – либо выберите «Фиксированное окно» в EVE.", "Cada cliente ocupa todo el monitor. Haz clic en su vista o usa un atajo para traerlo al frente. También activa «Sin bordes» – o elige «Ventana fija» en EVE.", "每个客户端占满整个显示器。点击预览或按快捷键即可切到前台。同时会开启“无边框” – 也可在 EVE 中选择“固定窗口”。"]),
    ("lay.same_monitor_hint", ["Gleicher Monitor: Hauptfenster und Raster überlappen – dafür besser „Haupt + Seitenleiste“.", "Same monitor: main window and grid overlap – use “Main + sidebar” instead.", "Même écran : la fenêtre principale et la grille se chevauchent – préférez « Principal + barre latérale ».", "Один монитор: главное окно и сетка перекрываются – лучше «Главное + боковая панель».", "Mismo monitor: ventana principal y cuadrícula se solapan – mejor «Principal + barra lateral».", "同一显示器:主窗口与网格会重叠 – 建议改用“主窗口 + 侧栏”。"]),
    ("lay.apply_preset", ["Vorlage ins Profil übernehmen", "Apply preset to profile", "Appliquer le modèle au profil", "Применить шаблон к профилю", "Aplicar plantilla al perfil", "将模板应用到配置"]),
    ("lay.primary", ["(Haupt)", "(primary)", "(principal)", "(основной)", "(principal)", "(主)"]),
    ("lay.section_rules", ["Charakter-Regeln", "Character rules", "Règles par personnage", "Правила персонажей", "Reglas por personaje", "角色规则"]),
    ("lay.rules_hint", ["Die Reihenfolge bestimmt Durchschalten und Belegung der Nebenfenster. Hotkeys schalten direkt auf den Charakter.", "The order defines cycling and secondary slot assignment. Hotkeys switch directly to the character.", "L'ordre définit le cycle et l'attribution des emplacements. Les raccourcis basculent directement vers le personnage.", "Порядок задаёт переключение и распределение слотов. Горячие клавиши переключают прямо на персонажа.", "El orden define el ciclo y la asignación de posiciones. Los atajos cambian directamente al personaje.", "顺序决定切换顺序和副窗口分配。快捷键可直接切换到该角色。"]),
    ("lay.add_rule", ["+ Regel", "+ Rule", "+ Règle", "+ Правило", "+ Regla", "+ 规则"]),
    ("lay.add_logged_in", ["Eingeloggte Charaktere hinzufügen ({0})", "Add logged-in characters ({0})", "Ajouter les personnages connectés ({0})", "Добавить вошедших персонажей ({0})", "Añadir personajes conectados ({0})", "添加已登录角色({0})"]),
    ("prof.profile", ["Profil", "Profile", "Profil", "Профиль", "Perfil", "配置"]),
    ("prof.rename_hint", ["Profil umbenennen", "Rename profile", "Renommer le profil", "Переименовать профиль", "Renombrar perfil", "重命名配置"]),
    ("prof.new", ["Neu", "New", "Nouveau", "Новый", "Nuevo", "新建"]),
    ("prof.duplicate", ["Duplizieren", "Duplicate", "Dupliquer", "Дублировать", "Duplicar", "复制"]),
    ("prof.delete", ["Löschen", "Delete", "Supprimer", "Удалить", "Eliminar", "删除"]),
    ("prof.copy_suffix", ["Kopie", "Copy", "Copie", "Копия", "Copia", "副本"]),

    // ---------------------------------------------------------------- Vorschau
    ("pv.section_live", ["Live-Vorschau", "Live previews", "Aperçus en direct", "Живые превью", "Vistas en vivo", "实时预览"]),
    ("pv.enabled", ["Vorschaufenster anzeigen", "Show preview windows", "Afficher les aperçus", "Показывать окна превью", "Mostrar ventanas de vista previa", "显示预览窗口"]),
    ("pv.runtime_on", ["Zur Laufzeit eingeblendet.", "Currently shown.", "Actuellement affichés.", "Сейчас показаны.", "Mostradas ahora.", "当前已显示。"]),
    ("pv.runtime_off", ["Zur Laufzeit per Hotkey/Knopf ausgeblendet.", "Currently hidden via hotkey/button.", "Masqués par raccourci/bouton.", "Скрыты клавишей/кнопкой.", "Ocultas por atajo/botón.", "已通过快捷键/按钮隐藏。"]),
    ("pv.toggle", ["Ein/Aus umschalten", "Toggle on/off", "Activer/désactiver", "Вкл/выкл", "Activar/desactivar", "开/关"]),
    ("pv.usage", ["Linksklick = Client fokussieren · Rechtsklick = ins Hauptfenster tauschen · Ziehen = verschieben · Am Rand oder an der Ecke ziehen = Größe frei ändern. Position und Größe werden je Charakter gespeichert.", "Left click = focus client · Right click = swap into main window · Drag = move · Drag edge or corner = resize freely. Position and size are saved per character.", "Clic gauche = focus · Clic droit = fenêtre principale · Glisser = déplacer · Glisser un bord ou un coin = redimensionner. Position et taille sont enregistrées par personnage.", "ЛКМ – фокус · ПКМ – в главное окно · Перетаскивание – переместить · Край или угол – изменить размер. Позиция и размер сохраняются для каждого персонажа.", "Clic izquierdo = enfocar · Clic derecho = a la ventana principal · Arrastrar = mover · Arrastrar borde o esquina = redimensionar. Posición y tamaño se guardan por personaje.", "左键 = 聚焦客户端 · 右键 = 换到主窗口 · 拖动 = 移动 · 拖动边缘或角落 = 自由调整大小。位置和大小按角色保存。"]),
    ("pv.section_size", ["Standardgröße", "Default size", "Taille par défaut", "Размер по умолчанию", "Tamaño predeterminado", "默认大小"]),
    ("pv.size_hint", ["Gilt für alle Vorschauen, deren Größe du nicht selbst gezogen hast.", "Applies to all previews you haven't resized yourself.", "S'applique aux aperçus non redimensionnés manuellement.", "Для превью, размер которых вы не меняли вручную.", "Se aplica a las vistas que no redimensionaste tú.", "适用于所有未手动调整大小的预览。"]),
    ("pv.width", ["Breite", "Width", "Largeur", "Ширина", "Ancho", "宽度"]),
    ("pv.height", ["Höhe", "Height", "Hauteur", "Высота", "Alto", "高度"]),
    ("pv.opacity", ["Deckkraft", "Opacity", "Opacité", "Непрозрачность", "Opacidad", "不透明度"]),
    ("pv.section_display", ["Darstellung", "Display", "Affichage", "Отображение", "Visualización", "显示"]),
    ("pv.show_titles", ["Charaktername über dem Bild anzeigen", "Show character name above the image", "Afficher le nom au-dessus de l'image", "Показывать имя над изображением", "Mostrar el nombre sobre la imagen", "在画面上方显示角色名"]),
    ("pv.highlight", ["Aktiven Client farbig umranden", "Highlight the active client", "Encadrer le client actif", "Выделять активного клиента", "Resaltar el cliente activo", "高亮当前客户端"]),
    ("pv.hide_active", ["Vorschau des aktiven Clients ausblenden", "Hide the active client's preview", "Masquer l'aperçu du client actif", "Скрывать превью активного клиента", "Ocultar la vista del cliente activo", "隐藏当前客户端的预览"]),
    ("pv.only_eve", ["Nur anzeigen, wenn ein EVE-Client im Vordergrund ist", "Only show while an EVE client is in the foreground", "N'afficher que si un client EVE est au premier plan", "Показывать только когда EVE на переднем плане", "Mostrar solo con un cliente de EVE en primer plano", "仅在 EVE 客户端位于前台时显示"]),
    ("pv.section_arrange", ["Anordnung & Andocken", "Arrangement & snapping", "Disposition et aimantation", "Расположение и прилипание", "Disposición y anclaje", "排列与吸附"]),
    ("pv.snap", ["Beim Verschieben und Größe-Ziehen an anderen Vorschauen und am Bildschirmrand einrasten", "Snap to other previews and screen edges while moving and resizing", "Aimanter aux autres aperçus et aux bords de l'écran", "Прилипать к другим превью и краям экрана", "Ajustar a otras vistas y a los bordes de la pantalla", "移动和调整大小时吸附到其他预览和屏幕边缘"]),
    ("pv.snap_distance", ["Fangbereich", "Snap distance", "Distance d'aimantation", "Радиус прилипания", "Distancia de ajuste", "吸附距离"]),
    ("pv.gap", ["Abstand zwischen Vorschauen", "Gap between previews", "Espacement entre aperçus", "Отступ между превью", "Separación entre vistas", "预览间距"]),
    ("pv.auto_arrange", ["Automatische Anordnung", "Automatic arrangement", "Disposition automatique", "Авторасположение", "Disposición automática", "自动排列"]),
    ("pv.as_row", ["als Reihe", "as a row", "en ligne", "рядом", "en fila", "横排"]),
    ("pv.as_column", ["als Spalte", "as a column", "en colonne", "столбцом", "en columna", "竖排"]),
    ("pv.wrap_after", ["Umbruch nach", "Wrap after", "Retour après", "Перенос после", "Salto tras", "换行数量"]),
    ("pv.previews_suffix", [" Vorschauen", " previews", " aperçus", " превью", " vistas", " 个预览"]),
    ("pv.wrap_hint", ["0 = kein Umbruch", "0 = no wrap", "0 = pas de retour", "0 = без переноса", "0 = sin salto", "0 = 不换行"]),
    ("pv.start", ["Startpunkt", "Start point", "Point de départ", "Начальная точка", "Punto de inicio", "起点"]),
    ("pv.auto_hint", ["Automatisch angeordnet werden alle Vorschauen ohne gespeicherte Position – lückenlos ab dem Startpunkt.", "Previews without a saved position are arranged automatically – seamlessly from the start point.", "Les aperçus sans position enregistrée sont disposés automatiquement à partir du point de départ.", "Превью без сохранённой позиции располагаются автоматически от начальной точки.", "Las vistas sin posición guardada se colocan automáticamente desde el punto de inicio.", "未保存位置的预览会从起点开始自动紧密排列。"]),
    ("pv.rearrange", ["Alle neu anordnen", "Re-arrange all", "Tout réorganiser", "Расставить заново", "Reorganizar todo", "全部重新排列"]),
    ("pv.rearrange_hint", ["Löscht gespeicherte Positionen – alle Vorschauen reihen sich wieder ab dem Startpunkt auf.", "Clears saved positions – all previews line up from the start point again.", "Efface les positions – tous les aperçus repartent du point de départ.", "Сбрасывает позиции – превью снова выстраиваются от начальной точки.", "Borra las posiciones – todas las vistas vuelven al punto de inicio.", "清除已保存的位置 – 所有预览重新从起点排列。"]),
    ("pv.reset_sizes", ["Alle auf Standardgröße", "All to default size", "Tout à la taille par défaut", "Всем размер по умолчанию", "Todo al tamaño predeterminado", "全部恢复默认大小"]),
    ("pv.reset_sizes_hint", ["Löscht selbst gezogene Größen.", "Clears manually set sizes.", "Efface les tailles personnalisées.", "Сбрасывает заданные вручную размеры.", "Borra los tamaños personalizados.", "清除手动设置的大小。"]),
    ("pv.section_saved", ["Gespeicherte Vorschauen", "Saved previews", "Aperçus enregistrés", "Сохранённые превью", "Vistas guardadas", "已保存的预览"]),
    ("pv.none_saved", ["Noch keine – einfach eine Vorschau verschieben oder in der Größe ziehen.", "None yet – just move or resize a preview.", "Aucun – déplacez ou redimensionnez un aperçu.", "Пока нет – переместите или измените размер превью.", "Ninguna aún – mueve o redimensiona una vista.", "暂无 – 移动或调整一个预览的大小即可。"]),
    ("pv.pos", ["Pos. {0}, {1}", "Pos. {0}, {1}", "Pos. {0}, {1}", "Поз. {0}, {1}", "Pos. {0}, {1}", "位置 {0}, {1}"]),
    ("pv.reset", ["zurücksetzen", "reset", "réinitialiser", "сбросить", "restablecer", "重置"]),
    ("pv.minimized_note", ["Hinweis: Minimierte Clients liefern Windows kein Bild – ihre Vorschau bleibt schwarz, bis sie wiederhergestellt werden.", "Note: minimized clients provide no image – their preview stays black until restored.", "Remarque : un client réduit ne fournit pas d'image – son aperçu reste noir jusqu'à sa restauration.", "Свёрнутые клиенты не дают изображения – превью остаётся чёрным до восстановления.", "Nota: los clientes minimizados no dan imagen – su vista queda negra hasta restaurarlos.", "提示:最小化的客户端不提供画面 – 恢复前预览保持黑色。"]),

    // ---------------------------------------------------------------- Leistung
    ("perf.section_cpu", ["CPU – inaktive Clients drosseln", "CPU – throttle inactive clients", "CPU – brider les clients inactifs", "ЦП – ограничение неактивных клиентов", "CPU – limitar clientes inactivos", "CPU – 限制非活动客户端"]),
    ("perf.throttle", ["Clients im Hintergrund drosseln", "Throttle background clients", "Brider les clients en arrière-plan", "Ограничивать фоновые клиенты", "Limitar clientes en segundo plano", "限制后台客户端"]),
    ("perf.throttle_hint", ["Der Vordergrund-Client bekommt mehr CPU-Zeit, die übrigen laufen mit niedrigerer Priorität weiter (sie werden nicht angehalten).", "The foreground client gets more CPU time; the others keep running at lower priority (they are not paused).", "Le client actif obtient plus de temps CPU ; les autres continuent en priorité plus basse (sans pause).", "Активный клиент получает больше времени ЦП, остальные работают с пониженным приоритетом (не останавливаются).", "El cliente activo obtiene más CPU; los demás siguen con menor prioridad (no se pausan).", "前台客户端获得更多 CPU 时间,其余客户端以较低优先级继续运行(不会暂停)。"]),
    ("perf.active_prio", ["Priorität des aktiven Clients", "Priority of the active client", "Priorité du client actif", "Приоритет активного клиента", "Prioridad del cliente activo", "当前客户端优先级"]),
    ("perf.inactive_prio", ["Priorität der Hintergrund-Clients", "Priority of background clients", "Priorité des clients en arrière-plan", "Приоритет фоновых клиентов", "Prioridad de los clientes en segundo plano", "后台客户端优先级"]),
    ("perf.bg_cores", ["Kerne für Hintergrund-Clients", "Cores for background clients", "Cœurs des clients en arrière-plan", "Ядра для фоновых клиентов", "Núcleos para clientes en segundo plano", "后台客户端使用的核心"]),
    ("perf.bg_cores_hint", ["Tipp: Hintergrund-Clients auf die hinteren Kerne legen, dann hat der aktive Client die vorderen fast für sich. „alle“ = Kerne der Charakter-Regel verwenden.", "Tip: put background clients on the last cores so the active client nearly has the first ones to itself. “all” = use the character rule's cores.", "Astuce : placez les clients en arrière-plan sur les derniers cœurs. « tous » = cœurs de la règle du personnage.", "Совет: отдайте фоновым клиентам последние ядра. «все» = ядра из правила персонажа.", "Consejo: asigna los últimos núcleos a los clientes en segundo plano. «todos» = núcleos de la regla del personaje.", "提示:把后台客户端放在靠后的核心上,当前客户端几乎独占前面的核心。“全部” = 使用角色规则中的核心。"]),
    ("perf.rules_hint", ["Feste Priorität/Kerne pro Charakter stellst du im Tab „Layouts“ unter „Charakter-Regeln“ ein. Beim Beenden werden alle Clients zurückgesetzt.", "Fixed priority/cores per character are set in the “Layouts” tab under “Character rules”. All clients are reset on exit.", "Priorité/cœurs fixes par personnage : onglet « Dispositions », section « Règles par personnage ». Tout est réinitialisé à la fermeture.", "Приоритет/ядра для персонажа задаются во вкладке «Раскладки» → «Правила персонажей». При выходе всё сбрасывается.", "Prioridad/núcleos fijos por personaje: pestaña «Diseños», «Reglas por personaje». Todo se restablece al salir.", "每个角色的固定优先级/核心在“布局”页的“角色规则”中设置。退出时会恢复所有客户端。"]),
    ("perf.minimize", ["Client minimieren, sobald ein anderer EVE-Client aktiv wird", "Minimize a client as soon as another EVE client becomes active", "Réduire un client dès qu'un autre client EVE devient actif", "Сворачивать клиента, когда активируется другой", "Minimizar un cliente cuando otro cliente de EVE se active", "其他 EVE 客户端激活时最小化当前客户端"]),
    ("perf.minimize_warn", ["Achtung: Minimierte Clients zeichnen kaum noch (spart viel GPU/CPU), ihre Vorschau bleibt aber schwarz und sie passen nicht zu Layouts mit mehreren sichtbaren Fenstern.", "Note: minimized clients barely render (saves a lot of GPU/CPU), but their preview stays black and it doesn't suit layouts with several visible windows.", "Attention : un client réduit ne dessine presque plus (économise GPU/CPU), mais son aperçu reste noir et cela ne convient pas aux dispositions à plusieurs fenêtres visibles.", "Внимание: свёрнутые клиенты почти не рендерят (экономия GPU/ЦП), но превью чёрное и это не подходит для раскладок с несколькими видимыми окнами.", "Atención: los clientes minimizados apenas renderizan (ahorra GPU/CPU), pero su vista queda negra y no encaja con diseños de varias ventanas visibles.", "注意:最小化的客户端几乎不渲染(节省大量 GPU/CPU),但预览会变黑,也不适合多窗口同时可见的布局。"]),
    ("perf.section_ram", ["Arbeitsspeicher", "Memory", "Mémoire", "Память", "Memoria", "内存"]),
    ("perf.ram_warn", ["Warnung ab (pro Client)", "Warn above (per client)", "Alerte au-delà de (par client)", "Предупреждать свыше (на клиента)", "Avisar a partir de (por cliente)", "警告阈值(每个客户端)"]),
    ("perf.ram_warn_hint", ["0 = keine Warnung", "0 = no warning", "0 = aucune alerte", "0 = без предупреждения", "0 = sin aviso", "0 = 不警告"]),
    ("perf.sys_warn", ["Warnung, wenn System-RAM frei unter", "Warn when free system RAM below", "Alerte si RAM système libre sous", "Предупреждать, если свободной ОЗУ меньше", "Avisar si la RAM libre baja de", "系统可用内存低于此值时警告"]),
    ("perf.auto_trim", ["Speicher der Hintergrund-Clients automatisch freigeben, alle", "Automatically free background clients' memory every", "Libérer automatiquement la mémoire des clients en arrière-plan toutes les", "Автоматически освобождать память фоновых клиентов каждые", "Liberar automáticamente la memoria de los clientes en segundo plano cada", "自动释放后台客户端内存,间隔"]),
    ("perf.trim_now", ["Jetzt freigeben (alle außer aktivem Client)", "Free now (all except the active client)", "Libérer maintenant (sauf le client actif)", "Освободить сейчас (кроме активного)", "Liberar ahora (excepto el cliente activo)", "立即释放(当前客户端除外)"]),
    ("perf.trim_hint", ["„Freigeben“ leert den Arbeitsspeicher (Working Set) des Prozesses: Windows verschiebt selten genutzte Seiten in die Standby-Liste. Das hilft, wenn der RAM knapp wird. Braucht der Client die Daten wieder, lädt er sie nach – das kann kurz ruckeln. Deshalb nie beim aktiven Client im Kampf verwenden.", "“Free” empties the process working set: Windows moves rarely used pages to the standby list. This helps when RAM gets tight. If the client needs the data again it reloads it – which can stutter briefly. So never use it on the active client in combat.", "« Libérer » vide la mémoire de travail du processus : Windows déplace les pages peu utilisées vers la liste d'attente. Utile quand la RAM manque. Si le client en a de nouveau besoin, il les recharge – ce qui peut saccader brièvement. À éviter sur le client actif en combat.", "«Освободить» очищает рабочий набор процесса: Windows переносит редко используемые страницы в резервный список. Это помогает при нехватке ОЗУ. Если данные снова понадобятся, клиент подгрузит их – возможны краткие подтормаживания. Не используйте на активном клиенте в бою.", "«Liberar» vacía la memoria de trabajo del proceso: Windows mueve las páginas poco usadas a la lista en espera. Ayuda cuando falta RAM. Si el cliente vuelve a necesitarlas, las recarga – puede haber tirones breves. No lo uses en el cliente activo en combate.", "“释放”会清空进程的工作集:Windows 会把不常用的页面移到备用列表,在内存紧张时有帮助。客户端再次需要时会重新加载,可能短暂卡顿。因此不要在战斗中对当前客户端使用。"]),

    // ---------------------------------------------------------------- Hotkeys-Tab
    ("hkt.section_global", ["Globale Hotkeys", "Global hotkeys", "Raccourcis globaux", "Глобальные сочетания", "Atajos globales", "全局快捷键"]),
    ("hkt.only_eve", ["Hotkeys nur aktiv, wenn ein EVE-Client (oder EVE Multibox) im Vordergrund ist", "Hotkeys only active while an EVE client (or EVE Multibox) is in the foreground", "Raccourcis actifs seulement si un client EVE (ou EVE Multibox) est au premier plan", "Сочетания активны только когда EVE (или EVE Multibox) на переднем плане", "Atajos activos solo con un cliente de EVE (o EVE Multibox) en primer plano", "仅当 EVE 客户端(或 EVE Multibox)在前台时快捷键才生效"]),
    ("hkt.only_eve_hint", ["Empfohlen: Sonst blockieren die Tasten auch Browser, Discord usw. Nachteil: Von außerhalb von EVE kannst du nicht per Hotkey zurückschalten – dann die Vorschau anklicken.", "Recommended: otherwise the keys are blocked in browsers, Discord etc. Downside: you can't switch back via hotkey from outside EVE – click a preview instead.", "Recommandé : sinon les touches sont bloquées dans le navigateur, Discord, etc. Inconvénient : impossible de revenir par raccourci depuis l'extérieur d'EVE – cliquez sur un aperçu.", "Рекомендуется: иначе клавиши блокируются в браузере, Discord и т. д. Минус: вне EVE переключиться клавишей нельзя – кликните по превью.", "Recomendado: si no, las teclas quedan bloqueadas en el navegador, Discord, etc. Inconveniente: fuera de EVE no puedes volver con un atajo – haz clic en una vista.", "推荐开启:否则这些按键在浏览器、Discord 等程序中也会被占用。缺点:在 EVE 之外无法用快捷键切回 – 请点击预览。"]),
    ("hkt.help_next", ["Schaltet in Regel-Reihenfolge zum nächsten Client.", "Switches to the next client in rule order.", "Passe au client suivant selon l'ordre des règles.", "Переключает на следующего клиента по порядку правил.", "Cambia al siguiente cliente según el orden de reglas.", "按规则顺序切换到下一个客户端。"]),
    ("hkt.help_prev", ["Schaltet zum vorherigen Client.", "Switches to the previous client.", "Passe au client précédent.", "Переключает на предыдущего клиента.", "Cambia al cliente anterior.", "切换到上一个客户端。"]),
    ("hkt.help_swap", ["Aktiver Client ↔ Hauptfenster. Ist er schon Haupt, kommt der vorherige zurück.", "Active client ↔ main window. If it already is main, the previous one returns.", "Client actif ↔ fenêtre principale. S'il l'est déjà, le précédent revient.", "Активный клиент ↔ главное окно. Если он уже главный – возвращается предыдущий.", "Cliente activo ↔ ventana principal. Si ya lo es, vuelve el anterior.", "当前客户端 ↔ 主窗口。若已是主窗口,则换回上一个。"]),
    ("hkt.help_layout", ["Ordnet alle Clients nach dem aktiven Profil an.", "Arranges all clients according to the active profile.", "Dispose tous les clients selon le profil actif.", "Расставляет клиентов по активному профилю.", "Coloca todos los clientes según el perfil activo.", "按当前配置排列所有客户端。"]),
    ("hkt.help_toggle", ["Blendet alle Vorschaufenster aus bzw. ein.", "Hides or shows all preview windows.", "Masque ou affiche tous les aperçus.", "Скрывает или показывает все превью.", "Oculta o muestra todas las vistas.", "隐藏或显示所有预览窗口。"]),
    ("hkt.section_chars", ["Charakter-Hotkeys (aktives Profil)", "Character hotkeys (active profile)", "Raccourcis par personnage (profil actif)", "Сочетания персонажей (активный профиль)", "Atajos de personajes (perfil activo)", "角色快捷键(当前配置)"]),
    ("hkt.none_chars", ["Noch keine – im Tab „Layouts“ unter „Charakter-Regeln“ festlegen.", "None yet – set them in the “Layouts” tab under “Character rules”.", "Aucun – à définir dans « Dispositions » → « Règles par personnage ».", "Пока нет – задайте во вкладке «Раскладки» → «Правила персонажей».", "Ninguno aún – defínelos en «Diseños» → «Reglas por personaje».", "暂无 – 请在“布局”页的“角色规则”中设置。"]),
    ("hkt.section_problems", ["Probleme", "Problems", "Problèmes", "Проблемы", "Problemas", "问题"]),
    ("hkt.section_syntax", ["Schreibweise", "Syntax", "Syntaxe", "Синтаксис", "Sintaxis", "写法"]),
    ("hkt.syntax", ["Modifier: Ctrl/Strg, Alt, Shift/Umschalt, Win – verbunden mit „+“. Tasten: A–Z, 0–9, F1–F24, Num0–Num9, Tab, Space, Enter, Esc, PageUp, PageDown, Home, End, Insert, Delete, Up/Down/Left/Right, NumAdd, NumSub, NumMul, NumDiv.", "Modifiers: Ctrl, Alt, Shift, Win – joined with “+”. Keys: A–Z, 0–9, F1–F24, Num0–Num9, Tab, Space, Enter, Esc, PageUp, PageDown, Home, End, Insert, Delete, Up/Down/Left/Right, NumAdd, NumSub, NumMul, NumDiv.", "Modificateurs : Ctrl, Alt, Shift, Win – reliés par « + ». Touches : A–Z, 0–9, F1–F24, Num0–Num9, Tab, Space, Enter, Esc, PageUp, PageDown, Home, End, Insert, Delete, Up/Down/Left/Right, NumAdd, NumSub, NumMul, NumDiv.", "Модификаторы: Ctrl, Alt, Shift, Win – через «+». Клавиши: A–Z, 0–9, F1–F24, Num0–Num9, Tab, Space, Enter, Esc, PageUp, PageDown, Home, End, Insert, Delete, Up/Down/Left/Right, NumAdd, NumSub, NumMul, NumDiv.", "Modificadores: Ctrl, Alt, Shift, Win – unidos con «+». Teclas: A–Z, 0–9, F1–F24, Num0–Num9, Tab, Space, Enter, Esc, PageUp, PageDown, Home, End, Insert, Delete, Up/Down/Left/Right, NumAdd, NumSub, NumMul, NumDiv.", "修饰键:Ctrl、Alt、Shift、Win – 用“+”连接。按键:A–Z、0–9、F1–F24、Num0–Num9、Tab、Space、Enter、Esc、PageUp、PageDown、Home、End、Insert、Delete、Up/Down/Left/Right、NumAdd、NumSub、NumMul、NumDiv。"]),
    ("hkt.examples", ["Beispiele: Ctrl+F1 · Alt+Shift+2 · Ctrl+Num5 · Win+F12", "Examples: Ctrl+F1 · Alt+Shift+2 · Ctrl+Num5 · Win+F12", "Exemples : Ctrl+F1 · Alt+Shift+2 · Ctrl+Num5 · Win+F12", "Примеры: Ctrl+F1 · Alt+Shift+2 · Ctrl+Num5 · Win+F12", "Ejemplos: Ctrl+F1 · Alt+Shift+2 · Ctrl+Num5 · Win+F12", "示例:Ctrl+F1 · Alt+Shift+2 · Ctrl+Num5 · Win+F12"]),
    ("hkt.section_compliant", ["Regelkonform", "EULA-compliant", "Conforme à l'EULA", "Соответствует EULA", "Conforme al EULA", "符合 EULA"]),
    ("hkt.compliant", ["Jeder Hotkey löst genau eine Aktion in Windows aus (Fenster fokussieren, verschieben, tauschen). EVE Multibox sendet niemals Tastatur- oder Mauseingaben an EVE-Clients und vervielfältigt keine Eingaben. Ein registrierter Hotkey wird von Windows abgefangen und erreicht EVE nicht – wähle Kombinationen, die du im Spiel nicht brauchst.", "Each hotkey triggers exactly one action in Windows (focus, move or swap a window). EVE Multibox never sends keyboard or mouse input to EVE clients and never duplicates input. A registered hotkey is intercepted by Windows and does not reach EVE – choose combinations you don't need in game.", "Chaque raccourci déclenche une seule action dans Windows (focus, déplacement ou échange de fenêtre). EVE Multibox n'envoie jamais d'entrées clavier ou souris aux clients EVE et ne duplique aucune entrée. Un raccourci enregistré est intercepté par Windows et n'atteint pas EVE – choisissez des combinaisons inutilisées en jeu.", "Каждое сочетание вызывает ровно одно действие Windows (фокус, перемещение или смена окна). EVE Multibox никогда не отправляет ввод клавиатуры или мыши в клиенты EVE и не дублирует ввод. Зарегистрированное сочетание перехватывается Windows и не доходит до EVE – выбирайте сочетания, не нужные в игре.", "Cada atajo ejecuta exactamente una acción en Windows (enfocar, mover o cambiar una ventana). EVE Multibox nunca envía entradas de teclado o ratón a los clientes de EVE ni duplica entradas. Un atajo registrado lo intercepta Windows y no llega a EVE – elige combinaciones que no uses en el juego.", "每个快捷键只在 Windows 中触发一个操作(聚焦、移动或切换窗口)。EVE Multibox 从不向 EVE 客户端发送键盘或鼠标输入,也不复制输入。已注册的快捷键会被 Windows 拦截,不会传到 EVE – 请选择游戏中用不到的组合。"]),

    // ---------------------------------------------------------------- Einstellungen
    ("set.section_general", ["Allgemein", "General", "Général", "Общие", "General", "常规"]),
    ("set.language", ["Sprache", "Language", "Langue", "Язык", "Idioma", "语言"]),
    ("set.lang_auto", ["Automatisch (Systemsprache)", "Automatic (system language)", "Automatique (langue du système)", "Автоматически (язык системы)", "Automático (idioma del sistema)", "自动(跟随系统语言)"]),
    ("set.font_missing", ["Für Chinesisch wurde keine passende Windows-Schrift gefunden (Microsoft YaHei / SimSun).", "No suitable Windows font for Chinese was found (Microsoft YaHei / SimSun).", "Aucune police Windows adaptée au chinois n'a été trouvée (Microsoft YaHei / SimSun).", "Не найден подходящий шрифт Windows для китайского (Microsoft YaHei / SimSun).", "No se encontró una fuente de Windows adecuada para chino (Microsoft YaHei / SimSun).", "未找到适用于中文的 Windows 字体(Microsoft YaHei / SimSun)。"]),
    ("set.section_tray", ["Infobereich (Tray)", "System tray", "Zone de notification", "Трей", "Bandeja del sistema", "系统托盘"]),
    ("set.close_to_tray", ["Schließen (X) blendet das Dashboard in den Infobereich aus, statt das Programm zu beenden", "Closing (X) hides the dashboard to the tray instead of quitting", "Fermer (X) masque le tableau de bord dans la zone de notification au lieu de quitter", "Закрытие (X) сворачивает панель в трей вместо выхода", "Cerrar (X) oculta el panel en la bandeja en lugar de salir", "点击关闭(X)时隐藏到托盘而不是退出"]),
    ("set.start_minimized", ["Im Infobereich starten (Dashboard ausgeblendet)", "Start in the tray (dashboard hidden)", "Démarrer dans la zone de notification (tableau de bord masqué)", "Запускать в трее (панель скрыта)", "Iniciar en la bandeja (panel oculto)", "启动时最小化到托盘(隐藏仪表板)"]),
    ("set.tray_hint", ["Doppelklick auf das Symbol öffnet das Dashboard, Rechtsklick zeigt das Menü.", "Double-click the icon to open the dashboard, right-click for the menu.", "Double-cliquez sur l'icône pour ouvrir, clic droit pour le menu.", "Двойной клик по значку открывает панель, правый – меню.", "Doble clic en el icono abre el panel, clic derecho muestra el menú.", "双击图标打开仪表板,右键显示菜单。"]),
    ("set.section_autostart", ["Autostart", "Autostart", "Démarrage automatique", "Автозапуск", "Inicio automático", "开机启动"]),
    ("set.autostart", ["Mit Windows starten", "Start with Windows", "Lancer avec Windows", "Запускать вместе с Windows", "Iniciar con Windows", "随 Windows 启动"]),
    ("set.autostart_hint", ["Startet EVE Multibox nach der Anmeldung automatisch im Infobereich.", "Starts EVE Multibox in the tray after you sign in.", "Lance EVE Multibox dans la zone de notification après la connexion.", "Запускает EVE Multibox в трее после входа в систему.", "Inicia EVE Multibox en la bandeja al iniciar sesión.", "登录 Windows 后自动在托盘中启动 EVE Multibox。"]),
    ("set.autostart_failed", ["Autostart konnte nicht geändert werden: {0}", "Could not change autostart: {0}", "Impossible de modifier le démarrage automatique : {0}", "Не удалось изменить автозапуск: {0}", "No se pudo cambiar el inicio automático: {0}", "无法修改开机启动:{0}"]),
    ("set.section_files", ["Dateien", "Files", "Fichiers", "Файлы", "Archivos", "文件"]),
    ("set.config_file", ["Konfiguration", "Configuration", "Configuration", "Конфигурация", "Configuración", "配置文件"]),
    ("set.log_file", ["Protokoll", "Log file", "Journal", "Журнал", "Registro", "日志"]),
    ("set.open_folder", ["Ordner öffnen", "Open folder", "Ouvrir le dossier", "Открыть папку", "Abrir carpeta", "打开文件夹"]),
    ("set.section_about", ["Über", "About", "À propos", "О программе", "Acerca de", "关于"]),
    ("set.version", ["Version {0}", "Version {0}", "Version {0}", "Версия {0}", "Versión {0}", "版本 {0}"]),
    ("set.about_text", ["Kostenloses Multibox-Werkzeug für EVE Online – ohne Input-Broadcasting.", "Free multiboxing tool for EVE Online – without input broadcasting.", "Outil de multiboxing gratuit pour EVE Online – sans diffusion d'entrées.", "Бесплатный инструмент мультибоксинга для EVE Online – без трансляции ввода.", "Herramienta gratuita de multiboxing para EVE Online – sin retransmisión de entradas.", "免费的 EVE Online 多开工具 – 不含输入广播。"]),
    ("set.contact", ["Fragen oder etwas Seltsames? Einfach @[3THEL] LeggaSchnegge anschreiben.", "Questions or anything odd? Just contact @[3THEL] LeggaSchnegge.", "Des questions ou un souci ? Contactez simplement @[3THEL] LeggaSchnegge.", "Вопросы или что-то странное? Просто напишите @[3THEL] LeggaSchnegge.", "¿Preguntas o algo raro? Escribe a @[3THEL] LeggaSchnegge.", "有问题或异常?请联系 @[3THEL] LeggaSchnegge。"]),
    ("set.ccp", ["EVE Online und alle zugehörigen Marken sind Eigentum von CCP hf. Dieses Werkzeug ist nicht mit CCP verbunden.", "EVE Online and all related marks are the property of CCP hf. This tool is not affiliated with CCP.", "EVE Online et toutes les marques associées appartiennent à CCP hf. Cet outil n'est pas affilié à CCP.", "EVE Online и все связанные товарные знаки принадлежат CCP hf. Инструмент не связан с CCP.", "EVE Online y todas las marcas relacionadas son propiedad de CCP hf. Esta herramienta no está afiliada a CCP.", "EVE Online 及所有相关商标归 CCP hf. 所有。本工具与 CCP 无关。"]),

    // ---------------------------------------------------------------- Tray, Start, Absturz
    ("tray.show", ["Dashboard öffnen", "Open dashboard", "Ouvrir le tableau de bord", "Открыть панель", "Abrir panel", "打开仪表板"]),
    ("tray.quit", ["Beenden", "Quit", "Quitter", "Выход", "Salir", "退出"]),
    ("tray.tooltip", ["EVE Multibox – {0} Client(s)", "EVE Multibox – {0} client(s)", "EVE Multibox – {0} client(s)", "EVE Multibox – клиентов: {0}", "EVE Multibox – {0} cliente(s)", "EVE Multibox – {0} 个客户端"]),
    ("main.already_running", ["EVE Multibox läuft bereits.\n\nFalls kein Fenster zu sehen ist: im Task-Manager „eve-multibox.exe“ beenden und neu starten.", "EVE Multibox is already running.\n\nIf no window is visible: end “eve-multibox.exe” in Task Manager and start again.", "EVE Multibox est déjà lancé.\n\nSi aucune fenêtre n'est visible : terminez « eve-multibox.exe » dans le Gestionnaire des tâches et relancez.", "EVE Multibox уже запущен.\n\nЕсли окна не видно: завершите «eve-multibox.exe» в диспетчере задач и запустите снова.", "EVE Multibox ya se está ejecutando.\n\nSi no ves ninguna ventana: finaliza «eve-multibox.exe» en el Administrador de tareas y vuelve a iniciarlo.", "EVE Multibox 已在运行。\n\n如果看不到窗口:请在任务管理器中结束“eve-multibox.exe”后重新启动。"]),
    ("main.window_failed", ["Das Fenster konnte nicht geöffnet werden.\n\n{0}\n\nBitte den Grafiktreiber aktualisieren.\nProtokoll: {1}", "The window could not be opened.\n\n{0}\n\nPlease update your graphics driver.\nLog: {1}", "Impossible d'ouvrir la fenêtre.\n\n{0}\n\nVeuillez mettre à jour le pilote graphique.\nJournal : {1}", "Не удалось открыть окно.\n\n{0}\n\nОбновите драйвер видеокарты.\nЖурнал: {1}", "No se pudo abrir la ventana.\n\n{0}\n\nActualiza el controlador gráfico.\nRegistro: {1}", "无法打开窗口。\n\n{0}\n\n请更新显卡驱动。\n日志:{1}"]),
    ("main.libs_missing", ["Es fehlen System-Bibliotheken für das Fenster:\n  {0}\n\nInstallieren mit:\n  {1}\n\nDanach EVE Multibox erneut starten.", "System libraries for the window are missing:\n  {0}\n\nInstall them with:\n  {1}\n\nThen start EVE Multibox again.", "Des bibliothèques système nécessaires à la fenêtre manquent :\n  {0}\n\nInstallez-les avec :\n  {1}\n\nPuis relancez EVE Multibox.", "Не хватает системных библиотек для окна:\n  {0}\n\nУстановите их командой:\n  {1}\n\nЗатем снова запустите EVE Multibox.", "Faltan bibliotecas del sistema para la ventana:\n  {0}\n\nInstálalas con:\n  {1}\n\nLuego vuelve a iniciar EVE Multibox.", "缺少窗口所需的系统库:\n  {0}\n\n请用以下命令安装:\n  {1}\n\n然后重新启动 EVE Multibox。"]),
    ("crash.title", ["EVE Multibox – Fehler", "EVE Multibox – Error", "EVE Multibox – Erreur", "EVE Multibox – Ошибка", "EVE Multibox – Error", "EVE Multibox – 错误"]),
    // ---------------------------------------------------------------- Kommandozeile (Linux)
    ("cli.usage", ["Aufruf: eve-multibox [--install | --uninstall | --minimized | --version]\n  --install    Startmenü-Eintrag und Icon für diesen Benutzer anlegen\n  --uninstall  Startmenü-Eintrag, Icon und Autostart entfernen\n  --minimized  im Infobereich starten", "Usage: eve-multibox [--install | --uninstall | --minimized | --version]\n  --install    add menu entry and icon for this user\n  --uninstall  remove menu entry, icon and autostart\n  --minimized  start in the system tray", "Usage : eve-multibox [--install | --uninstall | --minimized | --version]\n  --install    ajouter l'entrée de menu et l'icône pour cet utilisateur\n  --uninstall  supprimer l'entrée de menu, l'icône et le démarrage auto\n  --minimized  démarrer dans la zone de notification", "Использование: eve-multibox [--install | --uninstall | --minimized | --version]\n  --install    добавить пункт меню и значок для этого пользователя\n  --uninstall  удалить пункт меню, значок и автозапуск\n  --minimized  запуск в трее", "Uso: eve-multibox [--install | --uninstall | --minimized | --version]\n  --install    añadir entrada de menú e icono para este usuario\n  --uninstall  quitar entrada de menú, icono e inicio automático\n  --minimized  iniciar en la bandeja", "用法:eve-multibox [--install | --uninstall | --minimized | --version]\n  --install    为当前用户添加菜单项和图标\n  --uninstall  删除菜单项、图标和开机启动\n  --minimized  启动时最小化到托盘"]),
    ("cli.installed", ["EVE Multibox wurde ins Startmenü eingetragen ({0}).", "EVE Multibox was added to the application menu ({0}).", "EVE Multibox a été ajouté au menu des applications ({0}).", "EVE Multibox добавлен в меню приложений ({0}).", "EVE Multibox se añadió al menú de aplicaciones ({0}).", "EVE Multibox 已添加到应用程序菜单({0})。"]),
    ("cli.uninstalled", ["Startmenü-Eintrag, Icon und Autostart wurden entfernt. Konfiguration bleibt in {0}.", "Menu entry, icon and autostart were removed. Configuration stays in {0}.", "Entrée de menu, icône et démarrage auto supprimés. La configuration reste dans {0}.", "Пункт меню, значок и автозапуск удалены. Конфигурация осталась в {0}.", "Se quitaron la entrada de menú, el icono y el inicio automático. La configuración queda en {0}.", "已删除菜单项、图标和开机启动。配置保留在 {0}。"]),
    ("cli.failed", ["Fehler: {0}", "Error: {0}", "Erreur : {0}", "Ошибка: {0}", "Error: {0}", "错误:{0}"]),
    ("cli.setcap", ["Optional für Priorität und Speicher-Freigabe (einmalig):", "Optional, for priority and memory freeing (once):", "Optionnel, pour la priorité et la libération de mémoire (une fois) :", "Необязательно, для приоритета и освобождения памяти (один раз):", "Opcional, para prioridad y liberar memoria (una vez):", "可选,用于优先级和内存释放(只需一次):"]),

    // ---------------------------------------------------------------- Linux
    ("set.autostart_linux", ["Beim Anmelden starten", "Start on login", "Lancer à l'ouverture de session", "Запускать при входе в систему", "Iniciar al iniciar sesión", "登录时启动"]),
    ("set.autostart_hint_linux", ["Legt ~/.config/autostart/eve-multibox.desktop an – EVE Multibox startet nach der Anmeldung im Infobereich.", "Creates ~/.config/autostart/eve-multibox.desktop – EVE Multibox starts in the tray after you log in.", "Crée ~/.config/autostart/eve-multibox.desktop – EVE Multibox démarre dans la zone de notification après la connexion.", "Создаёт ~/.config/autostart/eve-multibox.desktop – EVE Multibox запускается в трее после входа.", "Crea ~/.config/autostart/eve-multibox.desktop – EVE Multibox se inicia en la bandeja al iniciar sesión.", "创建 ~/.config/autostart/eve-multibox.desktop – 登录后 EVE Multibox 会在托盘中启动。"]),
    ("set.font_missing_linux", ["Für Chinesisch wurde keine passende Schrift gefunden – bitte „Noto Sans CJK“ installieren (z. B. Paket fonts-noto-cjk).", "No suitable font for Chinese was found – please install “Noto Sans CJK” (e.g. package fonts-noto-cjk).", "Aucune police adaptée au chinois – installez « Noto Sans CJK » (p. ex. paquet fonts-noto-cjk).", "Не найден шрифт для китайского – установите «Noto Sans CJK» (напр. пакет fonts-noto-cjk).", "No se encontró una fuente para chino – instala «Noto Sans CJK» (p. ej. paquete fonts-noto-cjk).", "未找到中文字体 – 请安装“Noto Sans CJK”(例如软件包 fonts-noto-cjk)。"]),
    ("set.section_display", ["Anzeige-System", "Display server", "Serveur d'affichage", "Графический сервер", "Servidor gráfico", "显示服务器"]),
    ("set.display_x11", ["X11 – alle Funktionen verfügbar.", "X11 – all features available.", "X11 – toutes les fonctions disponibles.", "X11 – все функции доступны.", "X11 – todas las funciones disponibles.", "X11 – 所有功能均可用。"]),
    ("set.display_xwayland", ["Wayland mit XWayland – EVE (Proton) läuft über XWayland, Vorschau und Layouts funktionieren. Hotkeys greifen nur, solange ein EVE-Fenster oder EVE Multibox aktiv ist.", "Wayland with XWayland – EVE (Proton) runs via XWayland, previews and layouts work. Hotkeys only work while an EVE window or EVE Multibox is active.", "Wayland avec XWayland – EVE (Proton) passe par XWayland, aperçus et dispositions fonctionnent. Les raccourcis ne marchent que si une fenêtre EVE ou EVE Multibox est active.", "Wayland с XWayland – EVE (Proton) работает через XWayland, превью и раскладки работают. Горячие клавиши действуют, только пока активно окно EVE или EVE Multibox.", "Wayland con XWayland – EVE (Proton) funciona vía XWayland, vistas y diseños funcionan. Los atajos solo funcionan con una ventana de EVE o EVE Multibox activa.", "Wayland + XWayland – EVE(Proton)通过 XWayland 运行,预览和布局可用。快捷键仅在 EVE 窗口或 EVE Multibox 处于活动状态时生效。"]),
    ("set.display_none", ["Kein X11/XWayland gefunden – EVE-Fenster können nicht verwaltet werden. Bitte XWayland aktivieren bzw. eine X11-Sitzung nutzen.", "No X11/XWayland found – EVE windows cannot be managed. Please enable XWayland or use an X11 session.", "Aucun X11/XWayland trouvé – impossible de gérer les fenêtres EVE. Activez XWayland ou utilisez une session X11.", "X11/XWayland не найден – окна EVE не управляются. Включите XWayland или используйте сеанс X11.", "No se encontró X11/XWayland – no se pueden gestionar las ventanas de EVE. Activa XWayland o usa una sesión X11.", "未找到 X11/XWayland – 无法管理 EVE 窗口。请启用 XWayland 或使用 X11 会话。"]),
    ("set.no_tray", ["Kein Infobereich gefunden – Schließen (X) beendet das Programm. (GNOME: Erweiterung „AppIndicator“ installieren.)", "No system tray found – closing (X) quits the program. (GNOME: install the “AppIndicator” extension.)", "Aucune zone de notification – fermer (X) quitte le programme. (GNOME : installez l'extension « AppIndicator ».)", "Трей не найден – закрытие (X) завершает программу. (GNOME: установите расширение «AppIndicator».)", "No hay bandeja del sistema – cerrar (X) sale del programa. (GNOME: instala la extensión «AppIndicator».)", "未找到系统托盘 – 点击关闭(X)将退出程序。(GNOME:请安装“AppIndicator”扩展。)"]),
    ("perf.rights_ok", ["Erweiterte Prozessrechte vorhanden – Priorität und Speicher-Freigabe sind verfügbar.", "Extended process rights present – priority and memory freeing are available.", "Droits étendus présents – priorité et libération de mémoire disponibles.", "Расширенные права есть – приоритет и освобождение памяти доступны.", "Permisos ampliados presentes – prioridad y liberación de memoria disponibles.", "已具备扩展进程权限 – 可使用优先级和内存释放。"]),
    ("perf.rights_missing", ["Priorität ändern und Speicher freigeben brauchen unter Linux das Recht CAP_SYS_NICE. Einmal im Terminal ausführen und EVE Multibox neu starten (Kerne/Affinität funktionieren auch ohne):", "Changing priority and freeing memory need the CAP_SYS_NICE right on Linux. Run this once in a terminal and restart EVE Multibox (cores/affinity work without it):", "Sous Linux, changer la priorité et libérer la mémoire nécessitent le droit CAP_SYS_NICE. Exécutez ceci une fois dans un terminal puis relancez EVE Multibox (les cœurs fonctionnent sans) :", "Для смены приоритета и освобождения памяти в Linux нужно право CAP_SYS_NICE. Выполните один раз в терминале и перезапустите EVE Multibox (ядра работают и без него):", "En Linux, cambiar la prioridad y liberar memoria requieren el permiso CAP_SYS_NICE. Ejecuta esto una vez en una terminal y reinicia EVE Multibox (los núcleos funcionan sin él):", "在 Linux 上修改优先级和释放内存需要 CAP_SYS_NICE 权限。请在终端中运行一次以下命令并重启 EVE Multibox(核心/亲和性无需此权限):"]),
    ("perf.trim_hint_linux", ["„Freigeben“ lagert selten genutzten Speicher des Clients aus (MADV_PAGEOUT) – das wirkt nur mit Swap oder zram. Braucht der Client die Daten wieder, lädt er sie nach – das kann kurz ruckeln. Deshalb nie beim aktiven Client im Kampf verwenden.", "“Free” pages out rarely used client memory (MADV_PAGEOUT) – this only helps with swap or zram. If the client needs the data again it reloads it, which can stutter briefly. So never use it on the active client in combat.", "« Libérer » évince la mémoire peu utilisée du client (MADV_PAGEOUT) – utile seulement avec swap ou zram. Si le client en a besoin, il la recharge, ce qui peut saccader. À éviter sur le client actif en combat.", "«Освободить» выгружает редко используемую память клиента (MADV_PAGEOUT) – помогает только при наличии swap или zram. Если данные снова нужны, клиент подгрузит их – возможны подтормаживания. Не используйте на активном клиенте в бою.", "«Liberar» saca de memoria lo que el cliente usa poco (MADV_PAGEOUT) – solo sirve con swap o zram. Si el cliente lo necesita de nuevo lo recarga, lo que puede dar tirones. No lo uses en el cliente activo en combate.", "“释放”会将客户端不常用的内存换出(MADV_PAGEOUT)– 仅在有 swap 或 zram 时有效。客户端再次需要时会重新加载,可能短暂卡顿。因此不要在战斗中对当前客户端使用。"]),
    ("btn.copy", ["Kopieren", "Copy", "Copier", "Копировать", "Copiar", "复制"]),
    ("eng.trim_unavailable", ["Speicher freigeben braucht unter Linux CAP_SYS_NICE – siehe Tab „Leistung“.", "Freeing memory needs CAP_SYS_NICE on Linux – see the “Performance” tab.", "Libérer la mémoire nécessite CAP_SYS_NICE sous Linux – voir l'onglet « Performances ».", "Для освобождения памяти в Linux нужно CAP_SYS_NICE – см. вкладку «Производительность».", "Liberar memoria requiere CAP_SYS_NICE en Linux – ver la pestaña «Rendimiento».", "在 Linux 上释放内存需要 CAP_SYS_NICE – 请查看“性能”页。"]),
    ("eng.warn_rights_linux", ["{0}: Priorität konnte nicht gesetzt werden – siehe Tab „Leistung“ (CAP_SYS_NICE).", "{0}: priority could not be set – see the “Performance” tab (CAP_SYS_NICE).", "{0} : priorité impossible à définir – voir l'onglet « Performances » (CAP_SYS_NICE).", "{0}: не удалось задать приоритет – см. вкладку «Производительность» (CAP_SYS_NICE).", "{0}: no se pudo fijar la prioridad – ver la pestaña «Rendimiento» (CAP_SYS_NICE).", "{0}:无法设置优先级 – 请查看“性能”页(CAP_SYS_NICE)。"]),
    ("eng.no_x11", ["Kein X11/XWayland – EVE-Fenster können nicht verwaltet werden (siehe Tab „Einstellungen“).", "No X11/XWayland – EVE windows cannot be managed (see the “Settings” tab).", "Pas de X11/XWayland – impossible de gérer les fenêtres EVE (voir l'onglet « Paramètres »).", "Нет X11/XWayland – окна EVE не управляются (см. вкладку «Настройки»).", "Sin X11/XWayland – no se pueden gestionar las ventanas de EVE (ver la pestaña «Ajustes»).", "没有 X11/XWayland – 无法管理 EVE 窗口(请查看“设置”页)。"]),
    ("eng.no_composite", ["Der X-Server kann keine Live-Vorschau (Composite/Render fehlt).", "The X server cannot provide live previews (Composite/Render missing).", "Le serveur X ne permet pas les aperçus en direct (Composite/Render absent).", "X-сервер не поддерживает живые превью (нет Composite/Render).", "El servidor X no admite vistas en vivo (falta Composite/Render).", "X 服务器无法提供实时预览(缺少 Composite/Render)。"]),

    ("crash.text", ["EVE Multibox ist abgestürzt (Thread „{0}“):\n\n{1}\n\nDetails stehen in:\n{2}", "EVE Multibox crashed (thread “{0}”):\n\n{1}\n\nDetails are in:\n{2}", "EVE Multibox a planté (thread « {0} ») :\n\n{1}\n\nDétails dans :\n{2}", "EVE Multibox аварийно завершился (поток «{0}»):\n\n{1}\n\nПодробности в:\n{2}", "EVE Multibox se cerró inesperadamente (hilo «{0}»):\n\n{1}\n\nDetalles en:\n{2}", "EVE Multibox 崩溃(线程“{0}”):\n\n{1}\n\n详细信息见:\n{2}"]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Platzhalter `{0}`, `{1}` … eines Textes.
    fn placeholders(s: &str) -> HashSet<String> {
        (0..10)
            .map(|i| format!("{{{i}}}"))
            .filter(|p| s.contains(p.as_str()))
            .collect()
    }

    /// Keine doppelten Schlüssel, keine leeren Texte, überall dieselben Platzhalter.
    #[test]
    fn table_is_complete_and_consistent() {
        let mut seen = HashSet::new();
        for (key, texts) in TEXTS {
            assert!(seen.insert(*key), "doppelter Schlüssel: {key}");
            let expected = placeholders(texts[Lang::En.index()]);
            for (i, text) in texts.iter().enumerate() {
                assert!(!text.trim().is_empty(), "leerer Text: {key} [{i}]");
                assert_eq!(placeholders(text), expected, "Platzhalter weichen ab: {key} [{i}]");
            }
        }
    }

    /// Übersetzung, Platzhalter und unbekannte Schlüssel.
    #[test]
    fn lookup_and_format() {
        set_lang(Lang::De);
        assert_eq!(t("prof.new"), "Neu");
        assert_eq!(tf("set.version", &[&"1.2.3"]), "Version 1.2.3");
        set_lang(Lang::Zh);
        assert_eq!(t("prof.new"), "新建");
        assert_eq!(t("gibt.es.nicht"), "gibt.es.nicht");
        set_lang(Lang::En);
    }

    /// Windows-Sprachkennungen werden korrekt zugeordnet.
    #[test]
    fn windows_langids() {
        assert_eq!(Lang::from_windows_langid(0x0407), Lang::De);
        assert_eq!(Lang::from_windows_langid(0x0804), Lang::Zh);
        assert_eq!(Lang::from_windows_langid(0x0419), Lang::Ru);
        assert_eq!(Lang::from_windows_langid(0x0411), Lang::En); // Japanisch → Englisch
    }

    /// POSIX-Locales werden korrekt zugeordnet.
    #[test]
    fn posix_locales() {
        assert_eq!(Lang::from_locale("de_DE.UTF-8"), Lang::De);
        assert_eq!(Lang::from_locale("zh_CN.UTF-8"), Lang::Zh);
        assert_eq!(Lang::from_locale("C"), Lang::En);
        assert_eq!(Lang::from_locale(""), Lang::En);
    }
}
