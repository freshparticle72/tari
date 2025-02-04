@reorg
Feature: Reorgs

  @critical
  Scenario: Simple reorg to stronger chain
        # Chain 1
        #     Note: Use more than 1 base node to speed up the test
    Given I have a seed node SEED_B
    And I have a base node B connected to seed SEED_B
    And I have wallet WB connected to base node B
    And I have mining node BM connected to base node B and wallet WB
    And mining node BM mines 3 blocks with min difficulty 1 and max difficulty 50
        # Chain 2
        #     Note: Use more than 1 base node to speed up the test
    Given I have a seed node SEED_C
    And I have a base node C connected to seed SEED_C
    And I have wallet WC connected to base node C
    And I have mining node CM connected to base node C and wallet WC
    And mining node CM mines 10 blocks with min difficulty 51 and max difficulty 9999999999
        # Connect chain 1 and 2
    Then node B is at height 3
    And node C is at height 10
    Given I have a base node SA connected to nodes B,C
    Then node SA is at height 10
    And node B is at height 10
    And node C is at height 10

  @critical
  Scenario: Node rolls back reorg on invalid block
    Given I have a seed node SA
    And I have a base node B connected to seed SA
    When I mine 5 blocks on B
    Then node B is at height 5
    When I save the tip on B as BTip1
        # Try a few times to insert an invalid block
    And I mine a block on B at height 3 with an invalid MMR
    And I mine a block on B at height 3 with an invalid MMR
    And I mine a block on B at height 4 with an invalid MMR
    And I mine a block on B at height 4 with an invalid MMR
    Then node B is at tip BTip1

  @critical @reorg
  Scenario: Pruned mode reorg simple
    Given I have a base node NODE1 connected to all seed nodes
    And I have wallet WALLET1 connected to base node NODE1
    And I have mining node MINING1 connected to base node NODE1 and wallet WALLET1
    When mining node MINING1 mines 5 blocks with min difficulty 1 and max difficulty 20
    Then all nodes are at height 5
    Given I have a pruned node PNODE2 connected to node NODE1 with pruning horizon set to 5
    And I have wallet WALLET2 connected to base node PNODE2
    And I have mining node MINING2 connected to base node PNODE2 and wallet WALLET2
    When mining node MINING1 mines 4 blocks with min difficulty 1 and max difficulty 20
    Then all nodes are at height 9
    When mining node MINING2 mines 5 blocks with min difficulty 1 and max difficulty 20
    Then all nodes are at height 14
    When I stop node PNODE2
    When mining node MINING1 mines 3 blocks with min difficulty 1 and max difficulty 20
    And node NODE1 is at height 17
    And I stop node NODE1
    And I start PNODE2
    When mining node MINING2 mines 6 blocks with min difficulty 20 and max difficulty 1000000
    And node PNODE2 is at height 20
    When I start NODE1
    Then all nodes are at height 20

  @critical @reorg
  Scenario: Pruned mode reorg past horizon
    Given I have a base node NODE1 connected to all seed nodes
    And I have wallet WALLET1 connected to base node NODE1
    And I have mining node MINING1 connected to base node NODE1 and wallet WALLET1
    Given I have a base node NODE2 connected to node NODE1
    And I have wallet WALLET2 connected to base node NODE2
    And I have mining node MINING2 connected to base node NODE2 and wallet WALLET2
    When I mine a block on NODE1 with coinbase CB1
    Then all nodes are at height 1
    And I stop node NODE1
    And mining node MINING2 mines 19 blocks with min difficulty 20 and max difficulty 1000000
    And node NODE2 is at height 20
    And I stop node NODE2
    When I start NODE1
    And mining node MINING1 mines 3 blocks with min difficulty 1 and max difficulty 20
    And node NODE1 is at height 4
    When I create a transaction TX1 spending CB1 to UTX1
    When I submit transaction TX1 to NODE1
    Then NODE1 has TX1 in MEMPOOL state
    And mining node MINING1 mines 6 blocks with min difficulty 1 and max difficulty 20
    And node NODE1 is at height 10
    Given I have a pruned node PNODE1 connected to node NODE1 with pruning horizon set to 5
    Then node PNODE1 is at height 10
    When I start NODE2
    Then all nodes are at height 20
        # Because TX1 should have been re_orged out we should be able to spend CB1 again
    When I create a transaction TX2 spending CB1 to UTX2
    When I submit transaction TX2 to PNODE1
    Then PNODE1 has TX2 in MEMPOOL state

  Scenario Outline: Massive multiple reorg
        #
        # Chain 1a:
        #   Mine X1 blocks (orphan_storage_capacity default set to 10)
        #
    Given I have a seed node SEED_A1
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_A1 connected to seed SEED_A1
    And I have a base node NODE_A2 connected to seed SEED_A1
    When I mine <X1> blocks on SEED_A1
    Then node NODE_A1 is at height <X1>
    Then node NODE_A2 is at height <X1>
        #
        # Chain 1b:
        #   Mine Y1 blocks (orphan_storage_capacity default set to 10)
        #
    And I have a seed node SEED_A2
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_A3 connected to seed SEED_A2
    And I have a base node NODE_A4 connected to seed SEED_A2
    When I mine <Y1> blocks on SEED_A2
    Then node NODE_A3 is at height <Y1>
    Then node NODE_A4 is at height <Y1>
        #
        # Connect Chain 1a and 1b
        #
    And I connect node NODE_A1 to node NODE_A3 and wait 1 seconds
    And I connect node NODE_A2 to node NODE_A4 and wait 1 seconds
    And I connect node SEED_A1 to node SEED_A2 and wait <SYNC_TIME> seconds
    Then node SEED_A1 is at the same height as node SEED_A2
    When I mine 10 blocks on SEED_A1
    Then node SEED_A2 is at the same height as node SEED_A1
    Then node NODE_A1 is at the same height as node SEED_A1
    Then node NODE_A2 is at the same height as node SEED_A1
    Then node NODE_A3 is at the same height as node SEED_A1
    Then node NODE_A4 is at the same height as node SEED_A1
        #
        # Chain 2a:
        #   Mine X2 blocks (orphan_storage_capacity default set to 10)
        #
    Given I have a seed node SEED_B1
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_B1 connected to seed SEED_B1
    And I have a base node NODE_B2 connected to seed SEED_B1
    When I mine <X2> blocks on SEED_B1
    Then node NODE_B1 is at height <X2>
    Then node NODE_B2 is at height <X2>
        #
        # Chain 2b:
        #   Mine Y2 blocks (orphan_storage_capacity default set to 10)
        #
    And I have a seed node SEED_B2
        # Add multiple base nodes to ensure more robust comms
    And I have a base node NODE_B3 connected to seed SEED_B2
    And I have a base node NODE_B4 connected to seed SEED_B2
    When I mine <Y2> blocks on SEED_B2
    Then node NODE_B3 is at height <Y2>
    Then node NODE_B4 is at height <Y2>
        #
        # Connect Chain 2a and 2b
        #
    And I connect node NODE_B1 to node NODE_B3 and wait 1 seconds
    And I connect node NODE_B2 to node NODE_B4 and wait 1 seconds
    And I connect node SEED_B1 to node SEED_B2 and wait <SYNC_TIME> seconds
    Then node SEED_B1 is at the same height as node SEED_B2
    When I mine 10 blocks on SEED_B1
    Then node SEED_B2 is at the same height as node SEED_B1
    Then node NODE_B1 is at the same height as node SEED_B1
    Then node NODE_B2 is at the same height as node SEED_B1
    Then node NODE_B3 is at the same height as node SEED_B1
    Then node NODE_B4 is at the same height as node SEED_B1
        #
        # Connect Chain 1 and 2
        #
    And I connect node NODE_A1 to node NODE_B1 and wait 1 seconds
    And I connect node NODE_A3 to node NODE_B3 and wait 1 seconds
    And I connect node SEED_A1 to node SEED_B1 and wait <SYNC_TIME> seconds
    Then node SEED_A1 is at the same height as node SEED_B1
    When I mine 10 blocks on SEED_A1
    Then all nodes are at the same height as node SEED_A1
    @critical
    Examples:
        | X1     | Y1     | X2    | Y2   | SYNC_TIME |
        | 5      | 10     | 15    | 20   | 20        |

    @long-running
    Examples:
        | X1     | Y1     | X2    | Y2   | SYNC_TIME |
        | 100    | 125    | 150   | 175  | 30        |
        | 1010   | 1110   | 1210  | 1310 | 60        |
