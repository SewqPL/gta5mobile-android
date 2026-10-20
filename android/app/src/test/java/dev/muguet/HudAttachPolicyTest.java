package dev.muguet;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public class HudAttachPolicyTest {
    @Test public void fallsBackToTheNextWindowType() {
        assertEquals(1, HudAttachPolicy.nextType(0, 2));
        assertEquals(0, HudAttachPolicy.nextType(1, 2));
        assertEquals(2, HudAttachPolicy.nextType(1, 3));
    }

    @Test public void retriesFastThenSlowsDown() {
        assertEquals(200, HudAttachPolicy.delayMs(1));
        assertEquals(200, HudAttachPolicy.delayMs(9));
        assertEquals(1000, HudAttachPolicy.delayMs(10));
        assertEquals(1000, HudAttachPolicy.delayMs(50));
    }

    @Test public void logsTheFirstAttemptsAndThenEveryTwentieth() {
        assertTrue(HudAttachPolicy.logAttempt(1));
        assertTrue(HudAttachPolicy.logAttempt(4));
        assertFalse(HudAttachPolicy.logAttempt(5));
        assertTrue(HudAttachPolicy.logAttempt(20));
        assertFalse(HudAttachPolicy.logAttempt(21));
    }
}
