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

/// A blaze whose target stands 8 blocks above it rises toward it.
pub(super) async fn test_blaze_rises_to_target_above(helper: GameTestHelper) -> GameTestResult<()> {
    build_stone_floor(&helper, 5, 5).await?;
    // A ledge high above one corner, out of the blaze's way.
    helper
        .set_block_default(&BlockPos::new(4, 8, 4), &Block::STONE)
        .await?;
    helper
        .spawn_entity_at("minecraft:blaze", Vector3::new(0.5, 1.0, 0.5))
        .await?;
    helper
        .spawn_entity_at("minecraft:villager", Vector3::new(4.5, 9.0, 4.5))
        .await?;
    helper
        .start_sequence()
        // Hurt-by-target ignores a hit on the blaze's first tick (vanilla timestamp 0).
        .then_execute_after(2, |helper| async move {
            // The hurt-by-target goal makes the villager the blaze's target.
            helper
                .execute_command(
                    &BlockPos::new(0, 1, 0),
                    "damage @e[type=minecraft:blaze,limit=1] 1 minecraft:mob_attack by @e[type=minecraft:villager,limit=1]",
                )
                .await
                .map(drop)
        })
        .then_wait_until(|helper| async move {
            // 4 blocks up: the blaze cannot get there without flying.
            let risen = helper
                .execute_command(
                    &BlockPos::new(0, 4, 0),
                    "execute if entity @e[type=minecraft:blaze,dx=4,dy=8,dz=4]",
                )
                .await
                .unwrap_or(0);
            helper.assert_true(risen > 0, "the blaze did not rise toward its target")
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
