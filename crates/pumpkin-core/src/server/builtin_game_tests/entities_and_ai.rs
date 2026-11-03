use pumpkin_data::Block;
use pumpkin_gametest::{GameTestHelper, GameTestResult};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::build_stone_floor;

pub(super) async fn test_item_entities_merge(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 4, 4).await?;

    for x in 0..4 {
        for z in 0..4 {
            if x == 0 || x == 3 || z == 0 || z == 3 {
                helper
                    .set_block_default(&BlockPos::new(x, 1, z), &Block::STONE)
                    .await?;
            }
        }
    }

    helper
        .spawn_item_at("minecraft:diamond", 10, Vector3::new(1.5, 1.1, 1.5))
        .await?;
    helper
        .spawn_item_at("minecraft:diamond", 15, Vector3::new(1.5, 1.1, 1.5))
        .await?;

    let center = BlockPos::new(1, 1, 1);
    helper.succeed_when(move |helper| async move {
        helper.assert_entities_present("minecraft:item", 1).await?;
        helper
            .assert_item_entity_count("minecraft:diamond", &center, 2.0, 25)
            .await
    })?;
    Ok(())
}

/// How many striders touch the given block layer of the test area.
async fn striders_in_layer(helper: &GameTestHelper, y: i32) -> u32 {
    helper
        .execute_command(
            &BlockPos::new(0, y, 0),
            "execute if entity @e[type=minecraft:strider,dx=4,dy=0,dz=4]",
        )
        .await
        .unwrap_or(0)
}

/// A strider dropped into a two-deep lava pool stands on its surface.
pub(super) async fn test_strider_stands_on_lava(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    for y in 1..=2 {
        for x in 0..5 {
            for z in 0..5 {
                helper
                    .set_block_default(&BlockPos::new(x, y, z), &Block::LAVA)
                    .await?;
            }
        }
    }
    helper
        .spawn_entity_at("minecraft:strider", Vector3::new(2.5, 4.0, 2.5))
        .await?;

    helper
        .start_sequence()
        .then_idle(60)
        .then_execute(|helper| async move {
            // Standing on the top source block puts its feet at y 2.5.
            let on_surface = striders_in_layer(&helper, 2).await;
            let sunk = striders_in_layer(&helper, 1).await;
            helper.assert_true(
                on_surface == 1 && sunk == 0,
                format!("the strider is not on the lava surface ({on_surface} at the surface, {sunk} sunk)"),
            )
        })
        .then_succeed();
    Ok(())
}

pub(super) async fn test_zombie_pathfinds_to_villager(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 8).await?;

    for x in 0..5 {
        for z in 0..8 {
            if x == 0 || x == 4 || z == 0 || z == 7 {
                helper
                    .set_block_default(&BlockPos::new(x, 1, z), &Block::STONE)
                    .await?;
                helper
                    .set_block_default(&BlockPos::new(x, 2, z), &Block::STONE)
                    .await?;
            }
        }
    }

    helper
        .spawn_entity_at("minecraft:villager", Vector3::new(2.5, 1.0, 6.0))
        .await?;
    helper
        .spawn_entity_at("minecraft:husk", Vector3::new(2.5, 1.0, 1.5))
        .await?;

    helper.succeed_when(move |helper| async move {
        helper
            .assert_entity_present_in_box(
                "minecraft:husk",
                BlockPos::new(1, 1, 4),
                BlockPos::new(4, 3, 7),
            )
            .await
    })?;
    Ok(())
}
