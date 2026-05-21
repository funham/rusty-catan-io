use catan_agents::cli_command::{
    CliCommand, DevCardCommand, ParseCommandErrorKind, parse_cli_command,
};
use catan_core::gameplay::{
    game::command::RegularCommand,
    primitives::{
        build::Build,
        dev_card::{DevCardUsage, UsableDevCard},
        resource::Resource,
        trade::BankTradeKind,
    },
};

#[test]
fn parser_returns_none_for_empty_input() {
    assert_eq!(parse_cli_command("   ").unwrap(), None);
}

#[test]
fn parser_accepts_short_interactive_commands() {
    assert_eq!(
        parse_cli_command("kn").unwrap(),
        Some(CliCommand::DevCard(DevCardCommand::Interactive(
            UsableDevCard::Knight,
        )))
    );
    assert_eq!(
        parse_cli_command("bt").unwrap(),
        Some(CliCommand::InteractiveBankTrade)
    );
    assert_eq!(
        parse_cli_command("br").unwrap(),
        Some(CliCommand::InteractiveBuildRoad)
    );
}

#[test]
fn parser_accepts_full_regular_and_dev_card_commands() {
    assert!(matches!(
        parse_cli_command("build road 0 1").unwrap(),
        Some(CliCommand::Regular(RegularCommand::Build(Build::Road(_))))
    ));
    assert!(matches!(
        parse_cli_command("bank-trade ore brick G3").unwrap(),
        Some(CliCommand::Regular(RegularCommand::TradeWithBank(trade)))
            if trade.give == Resource::Ore
                && trade.take == Resource::Brick
                && trade.kind == BankTradeKind::PortGeneric
    ));
    assert!(matches!(
        parse_cli_command("use monopoly ore").unwrap(),
        Some(CliCommand::DevCard(DevCardCommand::Usage(
            DevCardUsage::Monopoly(Resource::Ore)
        )))
    ));
}

#[test]
fn parser_reports_malformed_and_unknown_commands() {
    let err = parse_cli_command("use knight nope").unwrap_err();
    assert_eq!(err.kind(), ParseCommandErrorKind::InvalidArguments);
    assert!(err.to_string().contains("invalid knight hex"));

    let err = parse_cli_command("xyzzy").unwrap_err();
    assert_eq!(err.kind(), ParseCommandErrorKind::UnknownCommand);
    assert!(err.to_string().contains("unknown command"));
}
