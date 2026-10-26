use clankerdiff_remote::testing::FakeRemotes;
use clankerdiff_remote::{
    InstanceState, Inventory, Lifecycle, POLL, POLL_SETTLING, RemoteSpec, Remotes,
};

#[test]
fn a_static_remote_has_one_running_instance_without_being_listed() {
    let mut inventory = Inventory::new(vec![static_remote("devbox")]);

    assert!(inventory.has_listed());
    assert_eq!(
        states(&inventory, "devbox"),
        [("devbox".to_owned(), InstanceState::Running)]
    );
    assert!(
        inventory.begin_listing().is_none(),
        "there is nothing to list"
    );
}

#[test]
fn a_listing_takes_in_each_remotes_instances_or_why_they_could_not_be_listed() {
    let remotes = FakeRemotes::default();
    remotes.add("patchwave", "mvm-1", InstanceState::Running, None);
    remotes.add("patchwave", "mvm-2", InstanceState::Ended, None);
    remotes.fail("broken", "AWS credential setup failed");
    let mut inventory = Inventory::new(vec![listed("patchwave"), listed("broken")]);
    assert!(!inventory.has_listed());
    assert_eq!(
        inventory.listing("patchwave"),
        None,
        "nothing is known before the first listing"
    );

    let again = list(&mut inventory, &remotes);

    assert!(!again);
    assert!(inventory.has_listed());
    assert_eq!(
        states(&inventory, "patchwave"),
        [("mvm-1".to_owned(), InstanceState::Running)]
    );
    assert_eq!(
        inventory.problem("broken"),
        Some("AWS credential setup failed")
    );
    assert_eq!(inventory.problem("patchwave"), None);
}

#[test]
fn a_listing_asked_for_while_one_runs_is_asked_for_again_when_it_is_finished() {
    let remotes = FakeRemotes::default();
    let mut inventory = Inventory::new(vec![listed("patchwave")]);
    let listing = inventory.begin_listing().expect("a listing");

    assert!(inventory.begin_listing().is_none());

    assert!(
        inventory.finish_listing(listing.run(&remotes)),
        "the second listing is asked for again"
    );
}

#[test]
fn a_change_shows_at_once_and_a_listing_that_overlaps_it_is_dropped() {
    let remotes = FakeRemotes::default();
    remotes.add("patchwave", "mvm-1", InstanceState::Running, None);
    let mut inventory = Inventory::new(vec![listed("patchwave")]);
    list(&mut inventory, &remotes);
    let stale = inventory.begin_listing().expect("a listing").run(&remotes);

    inventory.begin_change("patchwave", "mvm-1", Lifecycle::Suspend);
    let shown = states(&inventory, "patchwave");
    assert!(
        inventory.begin_listing().is_none(),
        "nothing is listed while a change is under way"
    );
    let again = inventory.finish_listing(stale);
    remotes
        .change(&listed("patchwave"), "mvm-1", Lifecycle::Suspend)
        .expect("the fake suspends");
    inventory.finish_change();
    list(&mut inventory, &remotes);

    assert_eq!(shown, [("mvm-1".to_owned(), InstanceState::Suspending)]);
    assert!(again, "the overlapping listing is asked for again");
    assert_eq!(
        states(&inventory, "patchwave"),
        [("mvm-1".to_owned(), InstanceState::Suspended)]
    );
}

#[test]
fn a_stopped_instance_shows_as_stopping_until_it_is_no_longer_listed() {
    let remotes = FakeRemotes::default();
    remotes.add("patchwave", "mvm-1", InstanceState::Running, None);
    let mut inventory = Inventory::new(vec![listed("patchwave")]);
    list(&mut inventory, &remotes);

    inventory.begin_change("patchwave", "mvm-1", Lifecycle::Stop);
    let shown = states(&inventory, "patchwave");
    remotes
        .change(&listed("patchwave"), "mvm-1", Lifecycle::Stop)
        .expect("the fake stops");
    inventory.finish_change();
    list(&mut inventory, &remotes);

    assert_eq!(shown, [("mvm-1".to_owned(), InstanceState::Stopping)]);
    assert_eq!(inventory.instances("patchwave"), []);
}

#[test]
fn a_launched_instance_shows_as_starting_until_it_is_listed() {
    let remotes = FakeRemotes::default();
    let mut inventory = Inventory::new(vec![listed("patchwave")]);
    let stale = inventory.begin_listing().expect("a listing").run(&remotes);
    let id = remotes
        .launch(&listed("patchwave"))
        .expect("the fake launches");

    inventory.launched("patchwave", &id);
    let shown = states(&inventory, "patchwave");
    let again = inventory.finish_listing(stale);
    list(&mut inventory, &remotes);

    assert_eq!(shown, [(id.clone(), InstanceState::Starting)]);
    assert!(again, "a listing from before the launch is dropped");
    assert_eq!(
        states(&inventory, "patchwave"),
        [(id, InstanceState::Running)]
    );
}

#[test]
fn listing_is_sooner_while_an_instance_is_on_its_way_to_another_state() {
    let remotes = FakeRemotes::default();
    remotes.add("patchwave", "mvm-1", InstanceState::Running, None);
    let mut inventory = Inventory::new(vec![listed("patchwave")]);
    list(&mut inventory, &remotes);
    let settled = inventory.poll_interval();

    inventory.begin_change("patchwave", "mvm-1", Lifecycle::Resume);

    assert_eq!(settled, POLL);
    assert_eq!(inventory.poll_interval(), POLL_SETTLING);
}

#[test]
fn replacing_the_remotes_keeps_the_instances_of_those_listed_the_same_way() {
    let remotes = FakeRemotes::default();
    remotes.add("patchwave", "mvm-1", InstanceState::Running, None);
    remotes.add("aether", "mvm-2", InstanceState::Running, None);
    let mut inventory = Inventory::new(vec![listed("patchwave"), listed("aether")]);
    list(&mut inventory, &remotes);
    let moved = RemoteSpec {
        dir: Some("/elsewhere".into()),
        ..listed("aether")
    };

    inventory.set_remotes(vec![listed("patchwave"), moved, static_remote("devbox")]);

    assert_eq!(
        states(&inventory, "patchwave"),
        [("mvm-1".to_owned(), InstanceState::Running)]
    );
    assert!(
        inventory.instances("aether").is_empty(),
        "a remote listed another way is listed again"
    );
    assert_eq!(
        states(&inventory, "devbox"),
        [("devbox".to_owned(), InstanceState::Running)]
    );
    assert!(!inventory.has_listed());
}

fn list(inventory: &mut Inventory, remotes: &FakeRemotes) -> bool {
    let listing = inventory.begin_listing().expect("a listing");
    inventory.finish_listing(listing.run(remotes))
}

fn states(inventory: &Inventory, remote: &str) -> Vec<(String, InstanceState)> {
    inventory
        .instances(remote)
        .iter()
        .map(|instance| (instance.id.clone(), instance.state))
        .collect()
}

fn listed(name: &str) -> RemoteSpec {
    RemoteSpec {
        list: Some(vec!["list".to_owned()]),
        ..RemoteSpec::named(name)
    }
}

fn static_remote(name: &str) -> RemoteSpec {
    RemoteSpec {
        shell: Some(vec!["ssh".to_owned(), name.to_owned()]),
        ..RemoteSpec::named(name)
    }
}
