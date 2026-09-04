-- Sound group names moved to Ukrainian along with the rest of the interface.
--
-- Groups are found by name when the sound pack installs, so a rename in the catalog
-- without this migration would not rename anything: it would create a second, empty
-- group beside the full one and leave the old name showing in the editor forever.
--
-- Renaming only where the new name is not already taken, so running this twice, or
-- running it against a database where someone renamed a group by hand, does nothing.

WITH renames(old_name, new_name) AS (
  VALUES
    ('Doors opening', 'Двері відчиняються'),
    ('Doors slamming', 'Двері грюкають'),
    ('Chests', 'Скрині'),
    ('Sword swings', 'Удари мечем'),
    ('Bows and arrows', 'Луки й стріли'),
    ('Thunder', 'Грім'),
    ('Fire', 'Вогонь'),
    ('Water', 'Вода'),
    ('Wolves', 'Вовки'),
    ('Small creatures', 'Дрібні істоти'),
    ('Footsteps', 'Кроки'),
    ('Magic', 'Магія'),
    ('Coins', 'Монети'),
    ('Breaking glass', 'Розбите скло'),
    ('Fireballs and explosions', 'Вогняні кулі й вибухи'),
    ('Healing magic', 'Магія зцілення'),
    ('Teleports and portals', 'Телепорти й портали'),
    ('Ice magic', 'Крижана магія'),
    ('Drawing a blade', 'Витягання клинка'),
    ('Blocks and parries', 'Блоки й парирування'),
    ('Armour', 'Обладунки'),
    ('A body hits the ground', 'Тіло падає на землю'),
    ('Screams', 'Крики'),
    ('Dragons', 'Дракони'),
    ('Horses', 'Коні'),
    ('Ghosts and spirits', 'Привиди й духи'),
    ('Bones and skeletons', 'Кістки й скелети'),
    ('Crows and ravens', 'Ворони й круки'),
    ('Drinking a potion', 'Питво зілля'),
    ('Dice', 'Кубики'),
    ('Keys and locks', 'Ключі й замки'),
    ('Scrolls and maps', 'Сувої й карти'),
    ('Wind', 'Вітер'),
    ('Rain', 'Дощ'),
    ('Bells', 'Дзвони'),
    ('Tavern', 'Таверна')
)
UPDATE sound_groups
SET name = (SELECT new_name FROM renames WHERE renames.old_name = sound_groups.name)
WHERE name IN (SELECT old_name FROM renames)
  AND NOT EXISTS (
    SELECT 1 FROM sound_groups AS taken
    WHERE taken.name = (SELECT new_name FROM renames WHERE renames.old_name = sound_groups.name)
  );
