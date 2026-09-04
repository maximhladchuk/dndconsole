//! Which Freesound sounds belong to which event.
//!
//! Every id here was chosen by hand from search results, by name, duration, rating and
//! download count, and every one is CC0 — that is checked by the manifest generator when
//! it resolves them, and by a test against the committed manifest, so a change of licence
//! on Freesound's side fails a build here rather than being shipped.
//!
//! Sounds are named by id rather than found by a search at install time. A search does
//! not rank the same way twice, and "whatever is top of the results today" is not
//! something to hand a new user unheard.

pub struct Theme {
    /// The event this group plays for. Must exist in `dndsound_detect::seed_events`.
    pub event_id: &'static str,
    /// The sound group's display name.
    pub group_name: &'static str,
    pub sound_ids: &'static [u64],
}

pub const THEMES: &[Theme] = &[
    Theme {
        event_id: "OPEN_DOOR",
        group_name: "Двері відчиняються",
        sound_ids: &[588509, 390123, 416962, 238420, 778457, 788015],
    },
    Theme {
        event_id: "DOOR_SLAM",
        group_name: "Двері грюкають",
        sound_ids: &[705170, 647649, 216879, 372877, 406595, 537737],
    },
    Theme {
        event_id: "CHEST_OPEN",
        group_name: "Скрині",
        // 536074 "Coin Gold Box Open" used to be here and is a coin sound, not a chest:
        // it fired for CHEST_OPEN in a live session and sounded wrong.
        sound_ids: &[202092, 771164, 573653, 573654, 364740],
    },
    Theme {
        event_id: "SWORD_SWING",
        group_name: "Удари мечем",
        sound_ids: &[475133, 475132, 733890, 485277, 326795, 326862],
    },
    Theme {
        event_id: "BOW_SHOT",
        group_name: "Луки й стріли",
        sound_ids: &[394185, 394178, 394180, 384915, 384916, 384908],
    },
    Theme {
        event_id: "THUNDER",
        group_name: "Грім",
        sound_ids: &[760212, 477839, 486274, 534023, 244693, 760213],
    },
    Theme {
        event_id: "FIRE",
        group_name: "Вогонь",
        // Torches are here rather than in an event of their own: FIRE already lists
        // "смолоскип" and "факел" among its keywords, and two events fighting over the
        // same nouns is how a sound stops being predictable.
        sound_ids: &[
            539972, 475879, 178886, 412558, 714566, 111331, 260554, 244926, 386385,
        ],
    },
    Theme {
        event_id: "WATER_SPLASH",
        group_name: "Вода",
        sound_ids: &[398039, 829676, 737233, 425140, 563858, 316590],
    },
    Theme {
        event_id: "WOLF_GROWL",
        group_name: "Вовки",
        sound_ids: &[399184, 399186, 753896, 342204, 378334, 472402],
    },
    Theme {
        event_id: "SMALL_CREATURE_SCURRY",
        group_name: "Дрібні істоти",
        sound_ids: &[288941, 536753, 800274, 445958, 428114],
    },
    Theme {
        event_id: "FOOTSTEPS",
        group_name: "Кроки",
        sound_ids: &[637555, 813622, 422686, 231851, 845027, 651516],
    },
    Theme {
        event_id: "MAGIC_CAST",
        group_name: "Магія",
        sound_ids: &[455341, 241809, 786290, 688048, 802726, 729536],
    },
    Theme {
        event_id: "COINS",
        group_name: "Монети",
        sound_ids: &[213979, 350869, 336576, 629985, 443334, 512216, 536074],
    },
    Theme {
        event_id: "GLASS_SHATTER",
        group_name: "Розбите скло",
        sound_ids: &[566448, 566446, 554564, 848312, 540451, 629625],
    },
    // --- Magic, split by kind. "Magic" on its own was too broad to be useful: a
    // fireball needs an explosion, not a shimmer.
    Theme {
        event_id: "FIREBALL",
        group_name: "Вогняні кулі й вибухи",
        sound_ids: &[105016, 431174, 186932, 267887, 522705, 442872],
    },
    Theme {
        event_id: "SPELL_HEAL",
        group_name: "Магія зцілення",
        sound_ids: &[562292, 346116, 407479, 715067, 351408, 471834],
    },
    Theme {
        event_id: "SPELL_TELEPORT",
        group_name: "Телепорти й портали",
        sound_ids: &[150950, 512217, 220202, 172207, 453391, 735062],
    },
    Theme {
        event_id: "SPELL_ICE",
        group_name: "Крижана магія",
        sound_ids: &[709888, 691005, 160420, 683180, 396447, 685253],
    },
    // --- Combat beyond the swing itself.
    Theme {
        event_id: "SWORD_UNSHEATHE",
        group_name: "Витягання клинка",
        sound_ids: &[320521, 107589, 581594, 423935, 175957, 175953],
    },
    Theme {
        event_id: "SHIELD_BLOCK",
        group_name: "Блоки й парирування",
        sound_ids: &[523760, 760636, 760633, 760634, 760635, 616493],
    },
    Theme {
        event_id: "ARMOR_CLANK",
        group_name: "Обладунки",
        sound_ids: &[505669, 587442, 443746, 345070, 494823, 185842],
    },
    Theme {
        event_id: "BODY_FALL",
        group_name: "Тіло падає на землю",
        sound_ids: &[417994, 325270, 504626, 325269, 346695, 346694],
    },
    Theme {
        event_id: "SCREAM",
        group_name: "Крики",
        sound_ids: &[169628, 219719, 203594, 221544, 243377, 445008],
    },
    // --- Creatures.
    Theme {
        event_id: "DRAGON_ROAR",
        group_name: "Дракони",
        sound_ids: &[85568, 398908, 145729, 442964, 466830, 532156],
    },
    Theme {
        event_id: "HORSE",
        group_name: "Коні",
        sound_ids: &[175356, 269571, 564626, 197212, 826753, 336701],
    },
    Theme {
        event_id: "GHOST_MOAN",
        group_name: "Привиди й духи",
        sound_ids: &[352508, 431979, 234044, 152721, 581090, 473525],
    },
    Theme {
        event_id: "BONES",
        group_name: "Кістки й скелети",
        sound_ids: &[202102, 202091, 249811, 192146, 392883, 144159],
    },
    Theme {
        event_id: "CROW",
        group_name: "Ворони й круки",
        sound_ids: &[66763, 56234, 75162, 67356, 512781, 716962],
    },
    // --- Items, which is where "Coins" alone used to be.
    Theme {
        event_id: "POTION_DRINK",
        group_name: "Питво зілля",
        sound_ids: &[574077, 445970, 531755, 320139, 133977, 368711],
    },
    Theme {
        event_id: "DICE_ROLL",
        group_name: "Кубики",
        sound_ids: &[177208, 629982, 353975, 353974, 162456, 94031],
    },
    Theme {
        event_id: "KEYS_LOCK",
        group_name: "Ключі й замки",
        sound_ids: &[267711, 390324, 119918, 187347, 418846, 565909],
    },
    Theme {
        event_id: "SCROLL_PAPER",
        group_name: "Сувої й карти",
        sound_ids: &[615337, 615334, 615336, 522185, 185051, 814247],
    },
    // --- Weather and places. These run longer than an effect; they are still one-shots,
    // played over narration rather than looped.
    Theme {
        event_id: "WIND",
        group_name: "Вітер",
        sound_ids: &[45642, 113173, 113175, 113176, 410369, 673323],
    },
    Theme {
        event_id: "RAIN",
        group_name: "Дощ",
        sound_ids: &[21189, 232731, 595717, 244028, 396484, 543649],
    },
    Theme {
        event_id: "BELL",
        group_name: "Дзвони",
        sound_ids: &[76405, 633208, 378799, 219047, 416992, 62963],
    },
    Theme {
        event_id: "TAVERN",
        group_name: "Таверна",
        sound_ids: &[415974, 509951, 457043, 424790, 710856, 222993],
    },
];

pub fn theme_for(event_id: &str) -> Option<&'static Theme> {
    THEMES.iter().find(|t| t.event_id == event_id)
}
