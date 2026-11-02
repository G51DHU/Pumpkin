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

/// Shulker at one end of a floor, a husk (which does not burn) 5 blocks away.
async fn place_shulker_and_husk(helper: &GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(helper, 8, 5).await?;
    helper
        .spawn_entity_at("minecraft:shulker", Vector3::new(1.5, 1.0, 2.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:husk", Vector3::new(6.5, 1.0, 2.5))
        .await?;
    Ok(())
}

const SHULKER_TARGETS_HUSK: &str = "execute as @e[type=minecraft:shulker,limit=1,sort=nearest] on target if entity @s[type=minecraft:husk]";

pub(super) async fn test_shulker_on_team_targets_monsters(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    place_shulker_and_husk(&helper).await?;
    let origin = BlockPos::new(1, 1, 2);
    // Fails harmlessly when an earlier run already added the team.
    let _ = helper
        .execute_command(&origin, "team add pumpkin_gametest_shulkers")
        .await;
    helper
        .execute_command(
            &origin,
            "team join pumpkin_gametest_shulkers @e[type=minecraft:shulker,limit=1,sort=nearest]",
        )
        .await?;

    helper.succeed_when(move |helper| async move {
        helper
            .execute_command(&BlockPos::new(1, 1, 2), SHULKER_TARGETS_HUSK)
            .await
            .map(|_| ())
    })?;
    Ok(())
}

pub(super) async fn test_shulker_without_team_ignores_monsters(
    helper: GameTestHelper,
) -> GameTestResult<()> {
    place_shulker_and_husk(&helper).await?;

    helper.succeed_on_tick_when(100, move |helper| async move {
        let targeting = helper
            .execute_command(&BlockPos::new(1, 1, 2), SHULKER_TARGETS_HUSK)
            .await
            .is_ok();
        helper.assert_false(targeting, "a shulker with no team targeted a monster")
    })?;
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
