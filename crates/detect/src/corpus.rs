//! The narration corpus, shared by every test suite that needs it.
//!
//! It lives in the library rather than in a test file so both the pure detector tests
//! and the semantic-layer tests measure precision against *the same* lines. Two copies
//! would drift, and the copy that drifts is always the one guarding the property you
//! care about.

/// One line of narration and what should happen.
pub struct Case {
    pub text: &'static str,
    pub expected: Option<&'static str>,
}

const fn fires(text: &'static str, event: &'static str) -> Case {
    Case {
        text,
        expected: Some(event),
    }
}

const fn silent(text: &'static str) -> Case {
    Case {
        text,
        expected: None,
    }
}

/// Narration that should trigger.
pub const POSITIVE: &[Case] = &[
    // --- OPEN_DOOR ---
    fires("You slowly push open the old wooden door.", "OPEN_DOOR"),
    fires("He opens the door and steps inside.", "OPEN_DOOR"),
    fires("The door slowly opens with a groan.", "OPEN_DOOR"),
    fires("The heavy door swings open before you.", "OPEN_DOOR"),
    fires("You open the door to the chapel.", "OPEN_DOOR"),
    fires("Ви повільно відчиняєте старі дерев'яні двері.", "OPEN_DOOR"),
    fires("Він різко відчиняє двері.", "OPEN_DOOR"),
    fires("Він прочинив двері.", "OPEN_DOOR"),
    fires("Ти штовхнув двері й вони піддалися.", "OPEN_DOOR"),
    fires("Вона відкриває двері до зали.", "OPEN_DOOR"),
    // --- SWORD_SWING ---
    fires("The knight swings his sword at you.", "SWORD_SWING"),
    fires("The orc swings his sword.", "SWORD_SWING"),
    fires("The bandit slashes at you with his sword.", "SWORD_SWING"),
    fires("The guard attacks you with a sword.", "SWORD_SWING"),
    fires("Гоблін б'є мечем.", "SWORD_SWING"),
    fires("Лицар замахується мечем на тебе.", "SWORD_SWING"),
    // Spoken at a real table and rejected: "вдарив" was missing from the action words,
    // so the sword was mentioned and, as far as the gate could tell, nothing was done
    // with it. One of the most ordinary verbs in the language.
    fires("Він підійшов до нього і вдарив мечем.", "SWORD_SWING"),
    fires("Гоблін розмахнувся мечем.", "SWORD_SWING"),
    fires("Орк рубонув мечем по щиту.", "SWORD_SWING"),
    // --- THUNDER ---
    fires("Thunder rolls across the valley.", "THUNDER"),
    fires("A peal of thunder shakes the windows.", "THUNDER"),
    fires("Lightning splits the sky above the castle.", "THUNDER"),
    fires("Гримить грім над замком.", "THUNDER"),
    fires("Раптом прогримів грім.", "THUNDER"),
    // --- WOLF_GROWL ---
    fires("The wolf growls from the treeline.", "WOLF_GROWL"),
    fires("A wolf snarls at you from the shadows.", "WOLF_GROWL"),
    fires("Вовк гарчить у темряві.", "WOLF_GROWL"),
    fires("Вовк загарчав із кущів.", "WOLF_GROWL"),
    fires("Вовки завили десь за пагорбом.", "WOLF_GROWL"),
    // --- DOOR_SLAM ---
    fires("He slams the door behind him.", "DOOR_SLAM"),
    fires("The door slams shut with a bang.", "DOOR_SLAM"),
    fires("Він грюкає дверима.", "DOOR_SLAM"),
    fires("Двері з грюкотом зачиняються.", "DOOR_SLAM"),
    // --- CHEST_OPEN ---
    fires("You lift the lid of the chest.", "CHEST_OPEN"),
    fires("The chest creaks open.", "CHEST_OPEN"),
    fires("Ти відчиняєш скриню.", "CHEST_OPEN"),
    fires("Він відкриває скриню з золотом.", "CHEST_OPEN"),
    // --- BOW_SHOT ---
    fires("The archer looses an arrow at you.", "BOW_SHOT"),
    fires("She draws her bow and fires.", "BOW_SHOT"),
    fires("Лучник пускає стрілу.", "BOW_SHOT"),
    fires("Він стріляє з лука.", "BOW_SHOT"),
    fires("Стріла влучає йому в груди.", "BOW_SHOT"),
    fires("І тут стріла пролітає над головою.", "BOW_SHOT"),
    // --- FIRE ---
    fires("The oil bursts into flames.", "FIRE"),
    fires("He lights the torch.", "FIRE"),
    fires("Раптом спалахує полум'я.", "FIRE"),
    fires("Він запалює факел.", "FIRE"),
    // --- WATER_SPLASH ---
    fires("The goblin falls into the water.", "WATER_SPLASH"),
    fires("She dives into the river.", "WATER_SPLASH"),
    fires("Гоблін падає у воду.", "WATER_SPLASH"),
    fires("Він стрибає у річку.", "WATER_SPLASH"),
    // --- FOOTSTEPS ---
    fires("You hear footsteps in the corridor.", "FOOTSTEPS"),
    fires("Footsteps approach from the dark.", "FOOTSTEPS"),
    fires("Ви чуєте кроки за дверима.", "FOOTSTEPS"),
    fires("Кроки наближаються.", "FOOTSTEPS"),
    // --- MAGIC_CAST ---
    fires("The wizard casts a spell.", "MAGIC_CAST"),
    fires("He chants an incantation under his breath.", "MAGIC_CAST"),
    fires("Маг читає заклинання.", "MAGIC_CAST"),
    fires("Вона промовляє заклинання.", "MAGIC_CAST"),
    // --- COINS ---
    fires("He tosses a coin to the innkeeper.", "COINS"),
    fires("The coins spill across the floor.", "COINS"),
    fires("Він кидає монету шинкарю.", "COINS"),
    fires("Монети розсипаються по підлозі.", "COINS"),
    // --- GLASS_SHATTER ---
    fires("The window breaks with a crash.", "GLASS_SHATTER"),
    fires("He smashes the bottle on the bar.", "GLASS_SHATTER"),
    fires("Скло розбивається на друзки.", "GLASS_SHATTER"),
    fires("Він розбиває пляшку об стіл.", "GLASS_SHATTER"),
    // --- SMALL_CREATURE_SCURRY ---
    fires(
        "Something small runs across the floor.",
        "SMALL_CREATURE_SCURRY",
    ),
    fires(
        "A tiny creature scurries past your boot.",
        "SMALL_CREATURE_SCURRY",
    ),
    fires(
        "A rat scurries away into the dark.",
        "SMALL_CREATURE_SCURRY",
    ),
    fires("Щур пробігає повз вас.", "SMALL_CREATURE_SCURRY"),
    // --- spoken into a live session and measured ---
    // Every one of these was said aloud at a real microphone. Several were rejected at
    // the time; they are here so a future change to the stemmer or the term lists
    // cannot quietly lose them again.
    fires("Пляшка розбилась.", "GLASS_SHATTER"),
    fires("Пляшка розлетілась.", "GLASS_SHATTER"),
    fires("Розбилося вікно.", "GLASS_SHATTER"),
    fires("Розбилося скло.", "GLASS_SHATTER"),
    fires("Так, ти відкриваєш скриню.", "CHEST_OPEN"),
    fires("Чудно, як меч здіймається над його головою.", "SWORD_SWING"),
    fires("Творить чари.", "MAGIC_CAST"),
    fires("В темному лісі вовк виє.", "WOLF_GROWL"),
    fires("Спалахнула блискавка.", "THUNDER"),
    fires("Рахує монети.", "COINS"),
    fires("Пірнув у річку.", "WATER_SPLASH"),
    fires("Почулися кроки.", "FOOTSTEPS"),
    fires("Факел загорівся.", "FIRE"),
    fires("Ти захлопнув двері.", "DOOR_SLAM"),
    fires("Маленький щур пробігає.", "SMALL_CREATURE_SCURRY"),
    // --- inflection coverage ---
    // Written to use verbs and nouns in forms that appear nowhere in `seed.rs`, to test
    // that a narrator does not have to say the listed phrase. Every one of these was a
    // miss at some point; several exposed the stemmer giving two forms of one word two
    // different keys.
    fires("Ти повільно прочиняєш важкі ворота.", "OPEN_DOOR"),
    fires("Вона розчахнула двері навстіж.", "OPEN_DOOR"),
    fires("Хвіртка зі скрипом відхилилася.", "OPEN_DOOR"),
    fires("Він відчинив люк у підлозі.", "OPEN_DOOR"),
    fires("Гоблін захлопнув двері перед твоїм носом.", "DOOR_SLAM"),
    fires("Ворота з гуркотом зачинилися.", "DOOR_SLAM"),
    fires("Вона причинила дверцята.", "DOOR_SLAM"),
    fires("Він відкидає кришку сундука.", "CHEST_OPEN"),
    fires("Скринька розкрилася.", "CHEST_OPEN"),
    fires("Він зриває замок зі скрині.", "CHEST_OPEN"),
    fires("Орк рубонув шаблею по щиту.", "SWORD_SWING"),
    fires("Лицар заносить клинок над тобою.", "SWORD_SWING"),
    fires("Він встромив меч у землю.", "SWORD_SWING"),
    // Drawing, not swinging. These two expected SWORD_SWING from before the draw had
    // an event of its own, and the expectation outlived the reason for it.
    fires("Розбійник вихопив катану.", "SWORD_UNSHEATHE"),
    fires("Вона оголила лезо.", "SWORD_UNSHEATHE"),
    fires("Стрілець натягнув тятиву.", "BOW_SHOT"),
    fires("Болт просвистів над головою.", "BOW_SHOT"),
    fires("Стріла вп'ялася в дерево.", "BOW_SHOT"),
    fires("Він вистрілив з арбалета.", "BOW_SHOT"),
    fires("Грім прокотився над долиною.", "THUNDER"),
    fires("Блискавка вдарила у вежу.", "THUNDER"),
    fires("Гроза загуркотіла вдалині.", "THUNDER"),
    fires("Небо розколола блискавиця.", "THUNDER"),
    fires("Смолоскип спалахнув.", "FIRE"),
    fires("Багаття зайнялося.", "FIRE"),
    fires("Вогнище палахкотить.", "FIRE"),
    fires("Він плюхнувся у ставок.", "WATER_SPLASH"),
    fires("Вона занурилася в озеро.", "WATER_SPLASH"),
    fires("Струмок хлюпнув під ногами.", "WATER_SPLASH"),
    fires("Вовчиця загарчала з темряви.", "WOLF_GROWL"),
    fires("Звір вишкірив зуби.", "WOLF_GROWL"),
    fires("Вовки завивають за пагорбом.", "WOLF_GROWL"),
    fires("Пацюк прошмигнув попід стіною.", "SMALL_CREATURE_SCURRY"),
    fires("Миша шкребеться в кутку.", "SMALL_CREATURE_SCURRY"),
    fires("Тварина метнулася в темряву.", "SMALL_CREATURE_SCURRY"),
    fires("Важкі чоботи затупотіли по каменю.", "FOOTSTEPS"),
    fires("Хода наближається коридором.", "FOOTSTEPS"),
    fires("Здалеку долинають кроки.", "FOOTSTEPS"),
    fires("Чаклун прошепотів закляття.", "MAGIC_CAST"),
    fires("Маг наклав закляття.", "MAGIC_CAST"),
    fires(
        "Вона вимовила останнє слово і чари спрацювали.",
        "MAGIC_CAST",
    ),
    fires("Він висипав жменю дукатів на стіл.", "COINS"),
    fires("Золото задзвеніло об підлогу.", "COINS"),
    fires("Гаманець впав і монети розсипалися.", "COINS"),
    fires("Він заплатив срібло.", "COINS"),
    fires("Шибка лопнула.", "GLASS_SHATTER"),
    fires("Дзеркало тріснуло.", "GLASS_SHATTER"),
    fires("Келих розлетівся на друзки.", "GLASS_SHATTER"),
    fires("Він потрощив пляшки об стіну.", "GLASS_SHATTER"),
    // --- The twenty-two events added alongside the sound packs. Every one appears in
    // both languages, and in the phrasing a table actually uses rather than the one the
    // term list was written from.
    fires("The wizard hurls a fireball into the corridor.", "FIREBALL"),
    fires("The fireball explodes among the goblins.", "FIREBALL"),
    fires("Чарівник кидає вогняну кулю.", "FIREBALL"),
    fires("Вогняна куля вибухає посеред залу.", "FIREBALL"),
    fires("Вибух відкинув його до стіни.", "FIREBALL"),
    fires("The cleric heals your wounds.", "SPELL_HEAL"),
    fires("Жрець зцілює твої рани.", "SPELL_HEAL"),
    fires("Рани затягуються на очах.", "SPELL_HEAL"),
    fires("Вона вилікувала пораненого.", "SPELL_HEAL"),
    fires("He teleports away before you can react.", "SPELL_TELEPORT"),
    fires("A portal tears open in front of you.", "SPELL_TELEPORT"),
    fires("Він телепортується геть.", "SPELL_TELEPORT"),
    fires("Перед вами відкривається портал.", "SPELL_TELEPORT"),
    fires("Ice spreads across the floor.", "SPELL_ICE"),
    fires("Крижаний промінь влучає в орка.", "SPELL_ICE"),
    fires("Мороз сковує йому ноги.", "SPELL_ICE"),
    fires("He draws his sword.", "SWORD_UNSHEATHE"),
    fires("She unsheathes her dagger.", "SWORD_UNSHEATHE"),
    fires("Він витягує меч із піхов.", "SWORD_UNSHEATHE"),
    fires("Вона дістає кинджал.", "SWORD_UNSHEATHE"),
    fires("Лицар оголив клинок.", "SWORD_UNSHEATHE"),
    fires("He blocks the blow with his shield.", "SHIELD_BLOCK"),
    fires("She parries the strike.", "SHIELD_BLOCK"),
    fires("Він блокує удар щитом.", "SHIELD_BLOCK"),
    fires("Вартовий відбив стрілу щитом.", "SHIELD_BLOCK"),
    fires("The guard's armour clanks as he turns.", "ARMOR_CLANK"),
    fires("Обладунки брязкають при кожному кроці.", "ARMOR_CLANK"),
    fires("Кольчуга задзвеніла в темряві.", "ARMOR_CLANK"),
    fires("The goblin drops dead.", "BODY_FALL"),
    fires("The body hits the floor.", "BODY_FALL"),
    fires("Гоблін падає замертво.", "BODY_FALL"),
    fires("Тіло гупнуло об підлогу.", "BODY_FALL"),
    fires("Вартовий осів на землю.", "BODY_FALL"),
    fires("He screams in pain.", "SCREAM"),
    fires("Він кричить від болю.", "SCREAM"),
    fires("З сусідньої кімнати пролунав крик.", "SCREAM"),
    fires("Вона заверещала і відсахнулася.", "SCREAM"),
    fires("The dragon roars above you.", "DRAGON_ROAR"),
    fires("Дракон реве над вами.", "DRAGON_ROAR"),
    fires("Чудовисько заричало в темряві.", "DRAGON_ROAR"),
    fires("The horse whinnies and rears.", "HORSE"),
    fires("Hooves clatter on the cobbles.", "HORSE"),
    fires("Кінь заіржав.", "HORSE"),
    fires("Копита стукотять по бруківці.", "HORSE"),
    fires("Вершники помчали дорогою.", "HORSE"),
    fires("A spirit moans in the dark.", "GHOST_MOAN"),
    fires("Привид пропливає крізь стіну.", "GHOST_MOAN"),
    fires("Примара завиває в каплиці.", "GHOST_MOAN"),
    fires("Bones crunch underfoot.", "BONES"),
    fires("The skeleton rises, bones rattling.", "BONES"),
    fires("Кістки хрустять під ногами.", "BONES"),
    fires("Скелет піднявся з могили.", "BONES"),
    fires("Crows caw from the gallows.", "CROW"),
    fires("Ворони закаркали над полем.", "CROW"),
    fires("Птахи зірвалися з дерев.", "CROW"),
    fires("He drinks the potion.", "POTION_DRINK"),
    fires("Він випиває зілля.", "POTION_DRINK"),
    fires("Вона залпом випила флакон.", "POTION_DRINK"),
    fires("Він ковтнув еліксир.", "POTION_DRINK"),
    // Table talk, not narration — but it is table talk that wants a sound, which is
    // exactly what the event is for. This line used to be a negative case, from before
    // there was a dice event to catch it.
    fires("Make a perception check, please.", "DICE_ROLL"),
    fires("Roll a d20 for me.", "DICE_ROLL"),
    fires("Кидай д20.", "DICE_ROLL"),
    fires("Кидаємо ініціативу.", "DICE_ROLL"),
    fires("Зроби рятівний кидок.", "DICE_ROLL"),
    fires("He turns the key in the lock.", "KEYS_LOCK"),
    fires("Він повертає ключ у замку.", "KEYS_LOCK"),
    fires("Ключі задзвеніли на поясі.", "KEYS_LOCK"),
    fires("Злодій зламав замок відмичкою.", "KEYS_LOCK"),
    fires("He unrolls the map on the table.", "SCROLL_PAPER"),
    fires("Він розгортає карту на столі.", "SCROLL_PAPER"),
    fires("Вона розгорнула сувій.", "SCROLL_PAPER"),
    fires("Пергамент зашурхотів.", "SCROLL_PAPER"),
    fires("The wind howls through the pass.", "WIND"),
    fires("Вітер виє в ущелині.", "WIND"),
    fires("Порив вітру збиває з ніг.", "WIND"),
    fires("It begins to rain.", "RAIN"),
    fires("Дощ тарабанить по даху.", "RAIN"),
    fires("Злива промочила вас до нитки.", "RAIN"),
    fires("The temple bell tolls.", "BELL"),
    fires("Дзвін б'є на храмі.", "BELL"),
    fires("Дзвони загули над містом.", "BELL"),
    fires("You push into the crowded tavern.", "TAVERN"),
    fires("Ви заходите в переповнену таверну.", "TAVERN"),
    fires("У корчмі гамірно від розмов.", "TAVERN"),
    // Written after the term lists were finished, using words none of them contained, and
    // then the lists were fixed until these fired. A corpus line that was written from a
    // term list only proves the list can read itself.
    fires("Маг метнув фаєрбол у натовп.", "FIREBALL"),
    fires("Цілителька заживила його рани.", "SPELL_HEAL"),
    fires("Крига скувала озеро.", "SPELL_ICE"),
    fires("Лицар вихопив шаблю з піхов.", "SWORD_UNSHEATHE"),
    fires("Вона підставила щит під сокиру.", "SHIELD_BLOCK"),
    fires("Лати гримнули, коли він розвернувся.", "ARMOR_CLANK"),
    fires("Орк повалився на землю.", "BODY_FALL"),
    fires("Хтось заволав у сусідній кімнаті.", "SCREAM"),
    fires("Дракон заревів так, що посипалося каміння.", "DRAGON_ROAR"),
    fires("Коні зацокотіли по бруківці.", "HORSE"),
    fires("Дух застогнав у підземеллі.", "GHOST_MOAN"),
    fires("Череп хруснув під чоботом.", "BONES"),
    fires("Круки закружляли над трупом.", "CROW"),
    fires("Він осушив флакон одним ковтком.", "POTION_DRINK"),
    fires("Кидайте перевірку на спритність.", "DICE_ROLL"),
    fires("Замок клацнув і піддався.", "KEYS_LOCK"),
    fires("Вона розгорнула пергамент на колінах.", "SCROLL_PAPER"),
    fires("Вітер завив між скель.", "WIND"),
    fires("Дощ полив як з відра.", "RAIN"),
    fires("Дзвони забили на сполох.", "BELL"),
    fires("Ви входите до корчми, всередині гамірно.", "TAVERN"),
    // --- Narration recorded at a real table, from the session log. Whisper's output,
    // not idealised prose: these are the lines that were spoken and missed.
    fires("Обладунки дзвонять.", "ARMOR_CLANK"),
    fires("Броня задзвонила.", "ARMOR_CLANK"),
    fires("Лати задзвонили.", "ARMOR_CLANK"),
    fires("Монети дзвонять у гаманці.", "COINS"),
    // The impersonal passive. A Dungeon Master narrating what has already happened
    // reaches for it constantly, and not one event could match it.
    fires("Скриню було відкрито.", "CHEST_OPEN"),
    fires("Двері було відчинено.", "OPEN_DOOR"),
    fires("Вікно було розбито.", "GLASS_SHATTER"),
    fires("Декілька коней проїхало повз.", "HORSE"),
    // Drawing a blade, which SWORD_SWING used to claim just as strongly.
    fires("Він витягнув свою шаблю.", "SWORD_UNSHEATHE"),
];

/// Narration that must stay silent. These are the cases that ruin a session when they
/// get them wrong.
pub const NEGATIVE: &[Case] = &[
    // The object is present but nothing is happening to it.
    silent("You see a sword lying on the table."),
    silent("A sword hangs on the wall above the fireplace."),
    silent("There is a door at the end of the corridor."),
    silent("The door is locked and will not budge."),
    silent("Меч лежить на столі."),
    // Metal rings, and a church bell is not what rang. BELL lists "дзвони" as an
    // object; "дзвонить" stems to the same thing, and one word used to answer for both.
    silent("Він дзвонить другові."),
    silent("Дзвін висить на дзвіниці."),
    silent("У кінці коридору ви бачите двері."),
    // Description of an image, not the thing itself.
    silent("There is a painted door on the far wall."),
    silent("На дверях намальований вовк."),
    // Memory and hypothesis.
    silent("You remember the sound of wolves from yesterday."),
    silent("Imagine the thunder if the storm reaches the village."),
    silent("If the wolf growls, roll initiative."),
    silent("Ви пам'ятаєте, як вовк гарчав минулої ночі."),
    // Ordinary narration with no event in it at all.
    silent("The tavern is warm and crowded tonight."),
    silent("You have four hit points left."),
    silent("Corner of the map, roughly two days ride north."),
    silent("Ви бачите старий кам'яний міст через річку."),
    silent("Кинь кубик на спритність."),
    // Said at a real table while testing. The object is there and so is a verb, but
    // neither of these is the event: drinking water is not a splash, and walking on
    // water is a miracle, not a footstep sound.
    silent("Всі ходять по воді і п'ють воду."),
    silent("He drinks water from the flask."),
    // "chest" the body part, which combat narration is full of. Deliberately without a
    // weapon verb: an arrow that hits someone *should* fire BOW_SHOT, and an earlier
    // version of this list called that a false positive when it was the right answer.
    silent("He clutches his chest and staggers."),
    silent("The wound in his chest is bleeding."),
    silent("Він тримається за груди."),
    // "bows" the gesture, not the weapon.
    silent("The innkeeper bows to you politely."),
    // Distance measured in paces, not the sound of walking.
    silent("The bridge is about twenty steps away."),
    silent("До мосту кілька кроків."),
    // A book about magic is not a spell being cast.
    silent("On the shelf you find a book of spells."),
    silent("На полиці лежить книга заклинань."),
    // A container of a drink, not breaking glass.
    silent("He hands you a glass of wine."),
];
