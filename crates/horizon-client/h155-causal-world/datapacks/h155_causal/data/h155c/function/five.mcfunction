execute if score @s h155c_id matches 1 run scoreboard players operation #seen_a h155c = #r h155c
execute if score @s h155c_id matches 2 run scoreboard players operation #seen_b h155c = #r h155c
scoreboard players add #calls h155c 1
scoreboard players operation #trace h155c *= #ten h155c
scoreboard players operation #trace h155c += @s h155c_id
return 5
