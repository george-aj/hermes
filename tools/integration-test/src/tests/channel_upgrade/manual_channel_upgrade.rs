use ibc_relayer::chain::requests::{IncludeProof, QueryChannelRequest, QueryHeight};
use ibc_relayer_types::core::{ics02_client::height::Height, ics04_channel::version::Version};
use ibc_test_framework::prelude::*;
use ibc_test_framework::relayer::channel::{
    assert_eventually_channel_established, assert_eventually_channel_upgrade_init,
    assert_eventually_channel_upgrade_try, init_channel_upgrade, try_channel_upgrade,
    ChannelUpgradeAssertionAttributes,
};

#[test]
fn test_channel_upgrade_init_handshake() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeInitHandshake)
}

#[test]
fn test_channel_upgrade_try_handshake() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeTryHandshake)
}

pub struct ChannelUpgradeInitHandshake;

impl TestOverrides for ChannelUpgradeInitHandshake {
    fn modify_test_config(&self, config: &mut TestConfig) {
        config.bootstrap_with_random_ids = false;
    }

    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.connections.enabled = true;

        config.mode.channels.enabled = false;
        config.mode.packets.enabled = false;
        config.mode.clients.enabled = false;
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for ChannelUpgradeInitHandshake {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!("Check that channels are both in OPEN State");

        assert_eventually_channel_established(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
        )?;

        let channel_end_a = chains
            .handle_a
            .query_channel(
                QueryChannelRequest {
                    port_id: channels.port_a.0.clone(),
                    channel_id: channels.channel_id_a.0.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map(|(channel_end, _)| channel_end)
            .map_err(|e| eyre!("Error querying ChannelEnd A: {e}"))?;

        let old_version = channel_end_a.version;
        let old_ordering = channel_end_a.ordering;
        let old_connection_hops = channel_end_a.connection_hops;

        let channel = channels.channel;
        let new_version = Version::ics20_with_fee();
        let new_ordering = None;
        let new_connection_hops = None;

        // Only Version is changed in this test.
        let upgrade_attrs = ChannelUpgradeAssertionAttributes::new(
            old_version,
            old_ordering,
            old_connection_hops.clone(),
            new_version.clone(),
            old_ordering,
            old_connection_hops,
        );

        let timeout_height = Height::new(
            ChainId::chain_version(chains.chain_id_a().0.to_string().as_str()),
            60,
        )
        .map_err(|e| eyre!("error creating height for timeout height: {e}"))?;

        info!("Initialise channel upgrade process...");

        let (channel_id_on_b, _) = init_channel_upgrade(
            &chains.handle_a,
            &chains.handle_b,
            channel,
            Some(new_version),
            new_ordering,
            new_connection_hops,
            Some(timeout_height),
            None,
        )?;

        info!("Check that the step ChanUpgradeInit was correctly executed...");

        assert_eventually_channel_upgrade_init(
            &chains.handle_b,
            &chains.handle_a,
            &channel_id_on_b.as_ref(),
            &channels.port_b.as_ref(),
            &upgrade_attrs,
        )?;

        Ok(())
    }
}

pub struct ChannelUpgradeTryHandshake;

impl TestOverrides for ChannelUpgradeTryHandshake {
    fn modify_test_config(&self, config: &mut TestConfig) {
        config.bootstrap_with_random_ids = false;
    }

    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.connections.enabled = true;

        config.mode.channels.enabled = false;
        config.mode.packets.enabled = false;
        config.mode.clients.enabled = false;
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for ChannelUpgradeTryHandshake {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!("Check that channels are both in OPEN State");

        assert_eventually_channel_established(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
        )?;

        let channel_end_a = chains
            .handle_a
            .query_channel(
                QueryChannelRequest {
                    port_id: channels.port_a.0.clone(),
                    channel_id: channels.channel_id_a.0.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map(|(channel_end, _)| channel_end)
            .map_err(|e| eyre!("Error querying ChannelEnd A: {e}"))?;

        let old_version = channel_end_a.version;
        let old_ordering = channel_end_a.ordering;
        let old_connection_hops = channel_end_a.connection_hops;

        let channel = channels.channel;
        let new_version = Version::ics20_with_fee();
        let new_ordering = None;
        let new_connection_hops = None;

        // Only Version is changed in this test.
        let init_upgrade_attrs = ChannelUpgradeAssertionAttributes::new(
            old_version,
            old_ordering,
            old_connection_hops.clone(),
            new_version.clone(),
            old_ordering,
            old_connection_hops.clone(),
        );

        // Only Version is changed in this test.
        let try_upgrade_attrs = ChannelUpgradeAssertionAttributes::new(
            new_version.clone(),
            old_ordering,
            old_connection_hops.clone(),
            new_version.clone(),
            old_ordering,
            old_connection_hops,
        );

        let timeout_height = Height::new(
            ChainId::chain_version(chains.chain_id_a().0.to_string().as_str()),
            60,
        )
        .map_err(|e| eyre!("error creating height for timeout height: {e}"))?;

        info!("Set channel in (INITUPGRADE, OPEN) state...");

        let (channel_id_on_b, _) = init_channel_upgrade(
            &chains.handle_a,
            &chains.handle_b,
            channel.clone(),
            Some(new_version),
            new_ordering,
            new_connection_hops,
            Some(timeout_height),
            None,
        )?;

        info!("Check that the step ChanUpgradeInit was correctly executed...");

        assert_eventually_channel_upgrade_init(
            &chains.handle_b,
            &chains.handle_a,
            &channel_id_on_b.as_ref(),
            &channels.port_b.as_ref(),
            &init_upgrade_attrs,
        )?;

        info!("Set channel in (INITUPGRADE, TRYUPGRADE) state...");

        //let (channel_id_on_b, _) =
        try_channel_upgrade(&chains.handle_a, &chains.handle_b, channel);

        assert_eventually_channel_upgrade_try(
            &chains.handle_b,
            &chains.handle_a,
            &channel_id_on_b.as_ref(),
            &channels.port_b.as_ref(),
            &try_upgrade_attrs,
        )?;

        Ok(())
    }
}