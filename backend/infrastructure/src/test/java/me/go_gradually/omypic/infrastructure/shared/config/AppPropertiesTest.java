package me.go_gradually.omypic.infrastructure.shared.config;

import me.go_gradually.omypic.application.stt.model.VadSettings;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;

class AppPropertiesTest {

    @Test
    void policyGetters_delegateToNestedProperties() {
        AppProperties properties = new AppProperties();
        properties.setDataDir("/tmp/omypic");

        properties.getStt().setMaxFileBytes(1234L);
        properties.getStt().setRetryMax(7);
        properties.getStt().getVad().setPrefixPaddingMs(111);
        properties.getStt().getVad().setSilenceDurationMs(222);
        properties.getStt().getVad().setThreshold(0.8);

        properties.getRag().setEmbeddingDim(32);
        properties.getRag().setMaxContextChunks(9);

        properties.getFeedback().setSummaryMaxChars(333);
        properties.getFeedback().setExampleMinRatio(0.7);
        properties.getFeedback().setExampleMaxRatio(1.5);
        properties.getFeedback().setWrongnoteSummaryMaxChars(88);
        properties.getFeedback().setWrongnoteWindowSize(33);
        properties.getIntegrations().getOpenai().setBaseUrl("http://openai.local");
        properties.getRealtime().setConversationModel("gpt-realtime");
        properties.getRealtime().setSttModel("gpt-4o-transcribe");
        properties.getRealtime().getVad().setPrefixPaddingMs(444);
        properties.getRealtime().getVad().setSilenceDurationMs(1555);
        properties.getRealtime().getVad().setThreshold(0.45);
        properties.getRealtime().setFeedbackModel("gpt-4o");
        properties.getRealtime().setFeedbackLanguage("en");
        properties.getRealtime().setTtsVoice("echo");
        properties.getRealtime().setRestDisabled(false);

        assertEquals("/tmp/omypic", properties.getDataDir());
        assertEquals(1234L, properties.getMaxFileBytes());
        assertEquals(7, properties.retryMax());
        VadSettings vadSettings = properties.getVadSettings();
        assertEquals(111, vadSettings.prefixPaddingMs());
        assertEquals(222, vadSettings.silenceDurationMs());
        assertEquals(0.8, vadSettings.threshold());

        assertEquals(9, properties.getMaxContextChunks());
        assertEquals(333, properties.getSummaryMaxChars());
        assertEquals(0.7, properties.getExampleMinRatio());
        assertEquals(1.5, properties.getExampleMaxRatio());
        assertEquals(88, properties.getWrongnoteSummaryMaxChars());
        assertEquals(33, properties.getWrongnoteWindowSize());
        assertEquals("http://openai.local", properties.getIntegrations().getOpenai().getBaseUrl());
        assertEquals("gpt-realtime", properties.realtimeConversationModel());
        assertEquals("gpt-4o-transcribe", properties.realtimeSttModel());
        assertEquals(444, properties.realtimeVadPrefixPaddingMs());
        assertEquals(1555, properties.realtimeVadSilenceDurationMs());
        assertEquals(0.45, properties.realtimeVadThreshold());
        assertEquals("gpt-4o", properties.realtimeFeedbackModel());
        assertEquals("en", properties.realtimeFeedbackLanguage());
        assertEquals("echo", properties.realtimeTtsVoice());
        assertFalse(properties.getRealtime().isRestDisabled());
    }
}
