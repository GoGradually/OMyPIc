package me.go_gradually.omypic.infrastructure.shared.config;

import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.rulebook.policy.RagPolicy;
import me.go_gradually.omypic.application.shared.policy.DataDirProvider;
import me.go_gradually.omypic.application.stt.model.VadSettings;
import me.go_gradually.omypic.application.stt.policy.SttPolicy;
import me.go_gradually.omypic.application.voice.policy.VoicePolicy;
import org.springframework.boot.context.properties.ConfigurationProperties;

@ConfigurationProperties(prefix = "omypic")
public class AppProperties implements DataDirProvider, SttPolicy, RagPolicy, FeedbackPolicy, VoicePolicy {
    private String dataDir;
    private Stt stt = new Stt();
    private Rag rag = new Rag();
    private Feedback feedback = new Feedback();
    private Voice voice = new Voice();
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

    public Voice getVoice() {
        return voice;
    }

    public void setVoice(Voice voice) {
        this.voice = voice;
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
    public String voiceSttModel() {
        return voice.getSttModel();
    }

    @Override
    public String voiceFeedbackModel() {
        return voice.getFeedbackModel();
    }

    @Override
    public String voiceFeedbackLanguage() {
        return voice.getFeedbackLanguage();
    }

    @Override
    public String voiceTtsModel() {
        return voice.getTtsModel();
    }

    @Override
    public String voiceTtsVoice() {
        return voice.getTtsVoice();
    }

    @Override
    public long voiceRecoveryRetentionMs() {
        return voice.getRecoveryRetentionMs();
    }

    @Override
    public int voiceStoppedContextMax() {
        return voice.getStoppedContextMax();
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
        private String provider = "fasttext";
        private int embeddingDim = 300;
        private int maxContextChunks = 4;
        private String modelPath = "";
        private String modelVersion = "cc.ko.300.vec.gz";
        private String modelSha256 = "9d71f0ae144e0f89dd233bfa7eca421be26bd1e5dd18e2cc56888a04be982d97";
        private String downloadUrl = "https://dl.fbaipublicfiles.com/fasttext/vectors-crawl/cc.ko.300.vec.gz";
        private int downloadTimeoutSeconds = 1800;
        private int downloadRetryMax = 2;
        private boolean allowHashFallback = false;
        private int modelMaxVocab = 200000;

        public String getProvider() {
            return provider;
        }

        public void setProvider(String provider) {
            this.provider = provider;
        }

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

        public String getModelPath() {
            return modelPath;
        }

        public void setModelPath(String modelPath) {
            this.modelPath = modelPath;
        }

        public String getModelVersion() {
            return modelVersion;
        }

        public void setModelVersion(String modelVersion) {
            this.modelVersion = modelVersion;
        }

        public String getModelSha256() {
            return modelSha256;
        }

        public void setModelSha256(String modelSha256) {
            this.modelSha256 = modelSha256;
        }

        public String getDownloadUrl() {
            return downloadUrl;
        }

        public void setDownloadUrl(String downloadUrl) {
            this.downloadUrl = downloadUrl;
        }

        public int getDownloadTimeoutSeconds() {
            return downloadTimeoutSeconds;
        }

        public void setDownloadTimeoutSeconds(int downloadTimeoutSeconds) {
            this.downloadTimeoutSeconds = downloadTimeoutSeconds;
        }

        public int getDownloadRetryMax() {
            return downloadRetryMax;
        }

        public void setDownloadRetryMax(int downloadRetryMax) {
            this.downloadRetryMax = downloadRetryMax;
        }

        public boolean isAllowHashFallback() {
            return allowHashFallback;
        }

        public void setAllowHashFallback(boolean allowHashFallback) {
            this.allowHashFallback = allowHashFallback;
        }

        public int getModelMaxVocab() {
            return modelMaxVocab;
        }

        public void setModelMaxVocab(int modelMaxVocab) {
            this.modelMaxVocab = modelMaxVocab;
        }
    }

    public static class Feedback {
        private int summaryMaxChars = 400;
        private double exampleMinRatio = 0.8;
        private double exampleMaxRatio = 1.2;
        private int wrongnoteSummaryMaxChars = 255;
        private int wrongnoteWindowSize = 100;

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

    public static class Voice {
        private String sttModel = "gpt-4o-mini-transcribe";
        private String feedbackModel = "gpt-5-nano";
        private String feedbackLanguage = "ko";
        private String ttsModel = "gpt-4o-mini-tts";
        private String ttsVoice = "alloy";
        private long recoveryRetentionMs = 600000L;
        private int stoppedContextMax = 1000;

        public String getSttModel() {
            return sttModel;
        }

        public void setSttModel(String sttModel) {
            this.sttModel = sttModel;
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

        public String getTtsModel() {
            return ttsModel;
        }

        public void setTtsModel(String ttsModel) {
            this.ttsModel = ttsModel;
        }

        public String getTtsVoice() {
            return ttsVoice;
        }

        public void setTtsVoice(String ttsVoice) {
            this.ttsVoice = ttsVoice;
        }

        public long getRecoveryRetentionMs() {
            return recoveryRetentionMs;
        }

        public void setRecoveryRetentionMs(long recoveryRetentionMs) {
            this.recoveryRetentionMs = recoveryRetentionMs;
        }

        public int getStoppedContextMax() {
            return stoppedContextMax;
        }

        public void setStoppedContextMax(int stoppedContextMax) {
            this.stoppedContextMax = stoppedContextMax;
        }
    }

    public static class Integrations {
        private OpenAi openai = new OpenAi();

        public OpenAi getOpenai() {
            return openai;
        }

        public void setOpenai(OpenAi openai) {
            this.openai = openai;
        }
    }

    public static class OpenAi {
        private String baseUrl = "https://api.openai.com";
        private boolean responsesEnabled = true;
        private int conversationRebaseTurns = 6;
        private Logging logging = new Logging();

        public String getBaseUrl() {
            return baseUrl;
        }

        public void setBaseUrl(String baseUrl) {
            this.baseUrl = baseUrl;
        }

        public boolean isResponsesEnabled() {
            return responsesEnabled;
        }

        public void setResponsesEnabled(boolean responsesEnabled) {
            this.responsesEnabled = responsesEnabled;
        }

        public int getConversationRebaseTurns() {
            return conversationRebaseTurns;
        }

        public void setConversationRebaseTurns(int conversationRebaseTurns) {
            this.conversationRebaseTurns = conversationRebaseTurns;
        }

        public Logging getLogging() {
            return logging;
        }

        public void setLogging(Logging logging) {
            this.logging = logging;
        }

        public static class Logging {
            private int responsePreviewChars = 1024;
            private boolean fullBody = false;
            private boolean logSuccessAtFine = true;

            public int getResponsePreviewChars() {
                return responsePreviewChars;
            }

            public void setResponsePreviewChars(int responsePreviewChars) {
                this.responsePreviewChars = responsePreviewChars;
            }

            public boolean isFullBody() {
                return fullBody;
            }

            public void setFullBody(boolean fullBody) {
                this.fullBody = fullBody;
            }

            public boolean isLogSuccessAtFine() {
                return logSuccessAtFine;
            }

            public void setLogSuccessAtFine(boolean logSuccessAtFine) {
                this.logSuccessAtFine = logSuccessAtFine;
            }
        }
    }

}
