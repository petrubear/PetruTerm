//! The delivery decision: every check in one pure function so routing is unit-testable.

use super::{Outcome, MAX_TEXT_CHARS};

/// Facts the router gathered about one message.
pub struct DecideInput<'a> {
    /// Sent by the user from the Agents list (bypasses acceptance and limits).
    pub from_user: bool,
    /// The sender's terminal was identified (always true for the user).
    pub sender_resolved: bool,
    /// Sender and target are the same terminal.
    pub same_terminal: bool,
    pub target_found: bool,
    pub target_alive: bool,
    pub target_accepting: bool,
    /// The name the sender used, for the outcome text.
    pub target_name: &'a str,
    /// Sanitized text.
    pub text: &'a str,
    /// Result of the rate limiter check (`Err(seconds)` = refused).
    pub rate: Result<(), u64>,
    pub conversation_ok: bool,
}

/// Why a message may not be delivered; `Ok(())` means deliver it. Order matters: cheap and
/// caller-side problems first, then the target, then acceptance, then limits.
pub fn decide(i: &DecideInput<'_>) -> Result<(), Outcome> {
    let name = || i.target_name.to_string();
    if i.text.trim().is_empty() {
        return Err(Outcome::Empty);
    }
    if i.text.chars().count() > MAX_TEXT_CHARS {
        return Err(Outcome::TooLong);
    }
    if !i.from_user && !i.sender_resolved {
        return Err(Outcome::SenderUnknown);
    }
    if !i.target_found {
        return Err(Outcome::UnknownName(name()));
    }
    if !i.from_user && i.same_terminal {
        return Err(Outcome::SelfMessage);
    }
    if !i.target_alive {
        return Err(Outcome::AgentGone(name()));
    }
    if !i.from_user && !i.target_accepting {
        return Err(Outcome::NotAccepting(name()));
    }
    if !i.from_user {
        if let Err(retry_after_s) = i.rate {
            return Err(Outcome::RateLimited { retry_after_s });
        }
        if !i.conversation_ok {
            return Err(Outcome::ConversationLimit);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_input() -> DecideInput<'static> {
        DecideInput {
            from_user: false,
            sender_resolved: true,
            same_terminal: false,
            target_found: true,
            target_alive: true,
            target_accepting: true,
            target_name: "kterm",
            text: "hello",
            rate: Ok(()),
            conversation_ok: true,
        }
    }

    #[test]
    fn a_normal_message_is_delivered() {
        assert_eq!(decide(&ok_input()), Ok(()));
    }

    #[test]
    fn each_refusal_has_its_outcome() {
        let mut i = ok_input();
        i.text = "  ";
        assert_eq!(decide(&i), Err(Outcome::Empty));
        let long = "a".repeat(MAX_TEXT_CHARS + 1);
        let mut i = ok_input();
        i.text = &long;
        assert_eq!(decide(&i), Err(Outcome::TooLong));
        let mut i = ok_input();
        i.sender_resolved = false;
        assert_eq!(decide(&i), Err(Outcome::SenderUnknown));
        let mut i = ok_input();
        i.target_found = false;
        assert_eq!(decide(&i), Err(Outcome::UnknownName("kterm".into())));
        let mut i = ok_input();
        i.same_terminal = true;
        assert_eq!(decide(&i), Err(Outcome::SelfMessage));
        let mut i = ok_input();
        i.target_alive = false;
        assert_eq!(decide(&i), Err(Outcome::AgentGone("kterm".into())));
        let mut i = ok_input();
        i.target_accepting = false;
        assert_eq!(decide(&i), Err(Outcome::NotAccepting("kterm".into())));
        let mut i = ok_input();
        i.rate = Err(12);
        assert_eq!(decide(&i), Err(Outcome::RateLimited { retry_after_s: 12 }));
        let mut i = ok_input();
        i.conversation_ok = false;
        assert_eq!(decide(&i), Err(Outcome::ConversationLimit));
    }

    #[test]
    fn order_unknown_before_acceptance_before_limits() {
        let mut i = ok_input();
        i.target_found = false;
        i.target_accepting = false;
        assert_eq!(decide(&i), Err(Outcome::UnknownName("kterm".into())));
        let mut i = ok_input();
        i.target_accepting = false;
        i.rate = Err(3);
        i.conversation_ok = false;
        assert_eq!(decide(&i), Err(Outcome::NotAccepting("kterm".into())));
        let mut i = ok_input();
        i.rate = Err(3);
        i.conversation_ok = false;
        assert_eq!(decide(&i), Err(Outcome::RateLimited { retry_after_s: 3 }));
    }

    /// Ask `decide` the way the router does, with live limiter state for the pair (1, 2).
    fn send(
        rates: &mut super::super::limits::RateLimiter,
        convos: &mut super::super::limits::Conversations,
        now: std::time::Instant,
    ) -> Result<(), Outcome> {
        let mut i = ok_input();
        i.rate = rates.check(now);
        i.conversation_ok = convos.allowed(1, 2, now);
        let r = decide(&i);
        if r.is_ok() {
            rates.record(now);
            convos.record(1, 2, now);
        }
        r
    }

    #[test]
    fn two_agents_answering_each_other_stop_at_ten_and_a_user_message_resets() {
        use super::super::limits::{Conversations, RateLimiter};
        use std::time::{Duration, Instant};
        let (mut rates, mut convos) = (RateLimiter::default(), Conversations::default());
        let t0 = Instant::now();
        // the rate limit (10/min) would also trip at 10, so spread the messages over 20 minutes
        // of pairs-less time: use a fresh limiter per message to isolate the conversation cap
        for n in 0..10u64 {
            let mut fresh = RateLimiter::default();
            let at = t0 + Duration::from_secs(n * 5);
            assert_eq!(send(&mut fresh, &mut convos, at), Ok(()), "message {n}");
        }
        let mut fresh = RateLimiter::default();
        assert_eq!(
            send(&mut fresh, &mut convos, t0 + Duration::from_secs(55)),
            Err(Outcome::ConversationLimit)
        );
        convos.reset_terminal(1); // the user sent a message to this agent
        assert_eq!(
            send(&mut rates, &mut convos, t0 + Duration::from_secs(56)),
            Ok(())
        );
    }

    #[test]
    fn eleven_messages_in_a_minute_from_one_sender_are_rate_limited() {
        use super::super::limits::{Conversations, RateLimiter};
        use std::time::{Duration, Instant};
        let (mut rates, mut convos) = (RateLimiter::default(), Conversations::default());
        let t0 = Instant::now();
        for n in 0..10u64 {
            convos.reset_terminal(1); // keep the conversation cap out of the way
            assert_eq!(
                send(&mut rates, &mut convos, t0 + Duration::from_secs(n)),
                Ok(())
            );
        }
        convos.reset_terminal(1);
        match send(&mut rates, &mut convos, t0 + Duration::from_secs(10)) {
            Err(Outcome::RateLimited { retry_after_s }) => assert_eq!(retry_after_s, 50),
            other => panic!("expected rate limited, got {other:?}"),
        }
    }

    #[test]
    fn user_bypasses_acceptance_and_limits_but_not_validity() {
        let mut i = ok_input();
        i.from_user = true;
        i.target_accepting = false;
        i.rate = Err(30);
        i.conversation_ok = false;
        i.same_terminal = true;
        assert_eq!(decide(&i), Ok(()));
        i.target_alive = false;
        assert_eq!(decide(&i), Err(Outcome::AgentGone("kterm".into())));
        let mut i = ok_input();
        i.from_user = true;
        i.sender_resolved = false;
        i.text = "";
        assert_eq!(decide(&i), Err(Outcome::Empty));
    }
}
