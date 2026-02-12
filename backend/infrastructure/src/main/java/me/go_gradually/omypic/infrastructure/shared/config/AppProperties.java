package me.go_gradually.omypic.infrastructure.shared.config;

import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.realtime.policy.RealtimePolicy;
import me.go_gradually.omypic.application.rulebook.policy.RagPolicy;
import me.go_gradually.omypic.application.shared.policy.DataDirProvider;
import me.go_gradually.omypic.application.stt.model.VadSettings;
import me.go_gradually.omypic.application.stt.policy.SttPolicy;
import org.springframework.boot.context.properties.ConfigurationProperties;

@ConfigurationProperties(prefix = "omypic")
public class AppProperties implements DataDirProvider, SttPolicy, RagPolicy, FeedbackPolicy, RealtimePolicy {
    private String dataDir;
    private Stt stt = new Stt();
    private Rag rag = new Rag();
    private Feedback feedback = new Feedback();
    private Realtime realtime = new Realtime();
    private Integrations integrations = new Integrations();

    public String getDataDir() {
        return dataDir;
    }

    public void setDataDir(String dataDir) {
        this.dataDir = dataDir;
    }

    public Stt getStt() {
        return stt;
    }

    public void setStt(Stt stt) {
        this.stt = stt;
    }

    public Rag getRag() {
        return rag;
    }

    public void setRag(Rag rag) {
        this.rag = rag;
    }

    public Feedback getFeedback() {
        return feedback;
    }

    public void setFeedback(Feedback feedback) {
        this.feedback = feedback;
    }

    public Realtime getRealtime() {
        return realtime;
    }

    public void setRealtime(Realtime realtime) {
        this.realtime = realtime;
    }

    public Integrations getIntegrations() {
        return integrations;
    }

    public void setIntegrations(Integrations integrations) {
        this.integrations = integrations;
    }

    @Override
    public long getMaxFileBytes() {
        return stt.getMaxFileBytes();
    }

    @Override
    public int retryMax() {
        return stt.getRetryMax();
    }

    @Override
    public VadSettings getVadSettings() {
        return new VadSettings(stt.getVad().getPrefixPaddingMs(),
                stt.getVad().getSilenceDurationMs(),
                stt.getVad().getThreshold());
    }

    @Override
    public int getMaxContextChunks() {
        return rag.getMaxContextChunks();
    }

    @Override
    public int getSummaryMaxChars() {
        return feedback.getSummaryMaxChars();
    }

    @Override
    public double getExampleMinRatio() {
        return feedback.getExampleMinRatio();
    }

    @Override
    public double getExampleMaxRatio() {
        return feedback.getExampleMaxRatio();
    }

    @Override
    public int getWrongnoteSummaryMaxChars() {
        return feedback.getWrongnoteSummaryMaxChars();
    }

    @Override
    public int getWrongnoteWindowSize() {
        return feedback.getWrongnoteWindowSize();
    }

    @Override
    public String realtimeConversationModel() {
        return realtime.getConversationModel();
    }

    @Override
    public String realtimeSttModel() {
        return realtime.getSttModel();
    }

    @Override
    public String realtimeFeedbackProvider() {
        return realtime.getFeedbackProvider();
    }

    @Override
    public String realtimeFeedbackModel() {
        return realtime.getFeedbackModel();
    }

    @Override
    public String realtimeFeedbackLanguage() {
        return realtime.getFeedbackLanguage();
    }

    @Override
    public String realtimeTtsVoice() {
        return realtime.getTtsVoice();
    }

    public static class Stt {
        private long maxFileBytes = 26214400L;
        private int maxDurationSeconds = 180;
        private int retryMax = 2;
        private Vad vad = new Vad();

        public long getMaxFileBytes() {
            return maxFileBytes;
        }

        public void setMaxFileBytes(long maxFileBytes) {
            this.maxFileBytes = maxFileBytes;
        }

        public int getMaxDurationSeconds() {
            return maxDurationSeconds;
        }

        public void setMaxDurationSeconds(int maxDurationSeconds) {
            this.maxDurationSeconds = maxDurationSeconds;
        }

        public int getRetryMax() {
            return retryMax;
        }

        public void setRetryMax(int retryMax) {
            this.retryMax = retryMax;
        }

        public Vad getVad() {
            return vad;
        }

        public void setVad(Vad vad) {
            this.vad = vad;
        }
    }

    public static class Vad {
        private int prefixPaddingMs = 300;
        private int silenceDurationMs = 200;
        private double threshold = 0.5;

        public int getPrefixPaddingMs() {
            return prefixPaddingMs;
        }

        public void setPrefixPaddingMs(int prefixPaddingMs) {
            this.prefixPaddingMs = prefixPaddingMs;
        }

        public int getSilenceDurationMs() {
            return silenceDurationMs;
        }

        public void setSilenceDurationMs(int silenceDurationMs) {
            this.silenceDurationMs = silenceDurationMs;
        }

        public double getThreshold() {
            return threshold;
        }

        public void setThreshold(double threshold) {
            this.threshold = threshold;
        }
    }

    public static class Rag {
        private int embeddingDim = 384;
        private int maxContextChunks = 4;

        public int getEmbeddingDim() {
            return embeddingDim;
        }

        public void setEmbeddingDim(int embeddingDim) {
            this.embeddingDim = embeddingDim;
        }

        public int getMaxContextChunks() {
            return maxContextChunks;
        }

        public void setMaxContextChunks(int maxContextChunks) {
            this.maxContextChunks = maxContextChunks;
        }
    }

    public static class Feedback {
        private int summaryMaxChars = 400;
        private double exampleMinRatio = 0.8;
        private double exampleMaxRatio = 1.2;
        private int wrongnoteSummaryMaxChars = 255;
        private int wrongnoteWindowSize = 30;

        public int getSummaryMaxChars() {
            return summaryMaxChars;
        }

        public void setSummaryMaxChars(int summaryMaxChars) {
            this.summaryMaxChars = summaryMaxChars;
        }

        public double getExampleMinRatio() {
            return exampleMinRatio;
        }

        public void setExampleMinRatio(double exampleMinRatio) {
            this.exampleMinRatio = exampleMinRatio;
        }

        public double getExampleMaxRatio() {
            return exampleMaxRatio;
        }

        public void setExampleMaxRatio(double exampleMaxRatio) {
            this.exampleMaxRatio = exampleMaxRatio;
        }

        public int getWrongnoteSummaryMaxChars() {
            return wrongnoteSummaryMaxChars;
        }

        public void setWrongnoteSummaryMaxChars(int wrongnoteSummaryMaxChars) {
            this.wrongnoteSummaryMaxChars = wrongnoteSummaryMaxChars;
        }

        public int getWrongnoteWindowSize() {
            return wrongnoteWindowSize;
        }

        public void setWrongnoteWindowSize(int wrongnoteWindowSize) {
            this.wrongnoteWindowSize = wrongnoteWindowSize;
        }
    }

    public static class Realtime {
        private String conversationModel = "gpt-realtime-mini";
        private String sttModel = "gpt-4o-mini-transcribe";
        private String feedbackProvider = "openai";
        private String feedbackModel = "gpt-4o-mini";
        private String feedbackLanguage = "ko";
        private String ttsVoice = "alloy";
        private boolean restDisabled = true;

        public String getConversationModel() {
            return conversationModel;
        }

        public void setConversationModel(String conversationModel) {
            this.conversationModel = conversationModel;
        }

        public String getSttModel() {
            return sttModel;
        }

        public void setSttModel(String sttModel) {
            this.sttModel = sttModel;
        }

        public String getFeedbackProvider() {
            return feedbackProvider;
        }

        public void setFeedbackProvider(String feedbackProvider) {
            this.feedbackProvider = feedbackProvider;
        }

        public String getFeedbackModel() {
            return feedbackModel;
        }

        public void setFeedbackModel(String feedbackModel) {
            this.feedbackModel = feedbackModel;
        }

        public String getFeedbackLanguage() {
            return feedbackLanguage;
        }

        public void setFeedbackLanguage(String feedbackLanguage) {
            this.feedbackLanguage = feedbackLanguage;
        }

        public String getTtsVoice() {
            return ttsVoice;
        }

        public void setTtsVoice(String ttsVoice) {
            this.ttsVoice = ttsVoice;
        }

        public boolean isRestDisabled() {
            return restDisabled;
        }

        public void setRestDisabled(boolean restDisabled) {
            this.restDisabled = restDisabled;
        }
    }

    public static class Integrations {
        private OpenAi openai = new OpenAi();
        private Gemini gemini = new Gemini();
        private Anthropic anthropic = new Anthropic();

        public OpenAi getOpenai() {
            return openai;
        }

        public void setOpenai(OpenAi openai) {
            this.openai = openai;
        }

        public Gemini getGemini() {
            return gemini;
        }

        public void setGemini(Gemini gemini) {
            this.gemini = gemini;
        }

        public Anthropic getAnthropic() {
            return anthropic;
        }

        public void setAnthropic(Anthropic anthropic) {
            this.anthropic = anthropic;
        }
    }

    public static class OpenAi {
        private String baseUrl = "https://api.openai.com";

        public String getBaseUrl() {
            return baseUrl;
        }

        public void setBaseUrl(String baseUrl) {
            this.baseUrl = baseUrl;
        }
    }

    public static class Gemini {
        private String baseUrl = "https://generativelanguage.googleapis.com";

        public String getBaseUrl() {
            return baseUrl;
        }

        public void setBaseUrl(String baseUrl) {
            this.baseUrl = baseUrl;
        }
    }

    public static class Anthropic {
        private String baseUrl = "https://api.anthropic.com";

        public String getBaseUrl() {
            return baseUrl;
        }

        public void setBaseUrl(String baseUrl) {
            this.baseUrl = baseUrl;
        }
    }
}
