package me.go_gradually.omypic.infrastructure.shared.config;

import me.go_gradually.omypic.application.stt.model.VadSettings;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

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
        properties.getRag().setProvider("fasttext");
        properties.getRag().setModelPath("/tmp/models/ko.vec.gz");
        properties.getRag().setModelVersion("ko.vec.gz");
        properties.getRag().setModelSha256("abc123");
        properties.getRag().setDownloadUrl("https://example.com/ko.vec.gz");
        properties.getRag().setDownloadTimeoutSeconds(45);
        properties.getRag().setDownloadRetryMax(3);
        properties.getRag().setAllowHashFallback(true);
        properties.getRag().setModelMaxVocab(321);

        properties.getFeedback().setSummaryMaxChars(333);
        properties.getFeedback().setExampleMinRatio(0.7);
        properties.getFeedback().setExampleMaxRatio(1.5);
        properties.getFeedback().setWrongnoteSummaryMaxChars(88);
        properties.getFeedback().setWrongnoteWindowSize(33);
        properties.getIntegrations().getOpenai().setBaseUrl("http://openai.local");
        properties.getIntegrations().getOpenai().getLogging().setResponsePreviewChars(777);
        properties.getIntegrations().getOpenai().getLogging().setFullBody(true);
        properties.getIntegrations().getOpenai().getLogging().setLogSuccessAtFine(false);
        properties.getVoice().setSttModel("gpt-4o-mini-transcribe");
        properties.getVoice().setFeedbackModel("gpt-4o-mini");
        properties.getVoice().setFeedbackLanguage("ko");
        properties.getVoice().setTtsModel("gpt-4o-mini-tts");
        properties.getVoice().setTtsVoice("alloy");
        properties.getVoice().setSilenceDurationMs(1666);

        assertEquals("/tmp/omypic", properties.getDataDir());
        assertEquals(1234L, properties.getMaxFileBytes());
        assertEquals(7, properties.retryMax());
        VadSettings vadSettings = properties.getVadSettings();
        assertEquals(111, vadSettings.prefixPaddingMs());
        assertEquals(222, vadSettings.silenceDurationMs());
        assertEquals(0.8, vadSettings.threshold());

        assertEquals(9, properties.getMaxContextChunks());
        assertEquals("fasttext", properties.getRag().getProvider());
        assertEquals("/tmp/models/ko.vec.gz", properties.getRag().getModelPath());
        assertEquals("ko.vec.gz", properties.getRag().getModelVersion());
        assertEquals("abc123", properties.getRag().getModelSha256());
        assertEquals("https://example.com/ko.vec.gz", properties.getRag().getDownloadUrl());
        assertEquals(45, properties.getRag().getDownloadTimeoutSeconds());
        assertEquals(3, properties.getRag().getDownloadRetryMax());
        assertTrue(properties.getRag().isAllowHashFallback());
        assertEquals(321, properties.getRag().getModelMaxVocab());
        assertEquals(333, properties.getSummaryMaxChars());
        assertEquals(0.7, properties.getExampleMinRatio());
        assertEquals(1.5, properties.getExampleMaxRatio());
        assertEquals(88, properties.getWrongnoteSummaryMaxChars());
        assertEquals(33, properties.getWrongnoteWindowSize());
        assertEquals("http://openai.local", properties.getIntegrations().getOpenai().getBaseUrl());
        assertEquals(777, properties.getIntegrations().getOpenai().getLogging().getResponsePreviewChars());
        assertTrue(properties.getIntegrations().getOpenai().getLogging().isFullBody());
        assertEquals(false, properties.getIntegrations().getOpenai().getLogging().isLogSuccessAtFine());
        assertEquals("gpt-4o-mini-transcribe", properties.voiceSttModel());
        assertEquals("gpt-4o-mini", properties.voiceFeedbackModel());
        assertEquals("ko", properties.voiceFeedbackLanguage());
        assertEquals("gpt-4o-mini-tts", properties.voiceTtsModel());
        assertEquals("alloy", properties.voiceTtsVoice());
        assertEquals(1666, properties.voiceSilenceDurationMs());
    }
}
