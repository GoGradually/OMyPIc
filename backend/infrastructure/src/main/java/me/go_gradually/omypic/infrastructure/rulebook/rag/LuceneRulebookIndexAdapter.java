package me.go_gradually.omypic.infrastructure.rulebook.rag;

import me.go_gradually.omypic.application.rulebook.port.EmbeddingPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookIndexPort;
import me.go_gradually.omypic.application.shared.policy.DataDirProvider;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import org.apache.lucene.analysis.standard.StandardAnalyzer;
import org.apache.lucene.document.*;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexWriter;
import org.apache.lucene.index.IndexWriterConfig;
import org.apache.lucene.search.IndexSearcher;
import org.apache.lucene.search.KnnVectorQuery;
import org.apache.lucene.search.ScoreDoc;
import org.apache.lucene.search.TopDocs;
import org.apache.lucene.store.Directory;
import org.apache.lucene.store.FSDirectory;
import org.springframework.stereotype.Component;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Properties;
import java.util.Set;

@Component
public class LuceneRulebookIndexAdapter implements RulebookIndexPort {
    private static final String META_FILE = "index-meta.properties";
    private static final String META_PROVIDER = "provider";
    private static final String META_MODEL_VERSION = "modelVersion";
    private static final String META_DIMENSION = "dimension";
    private final Path indexPath;
    private final EmbeddingPort embeddingService;
    private final Object indexLock = new Object();

    public LuceneRulebookIndexAdapter(DataDirProvider dataDirProvider, EmbeddingPort embeddingService) {
        this.indexPath = Path.of(dataDirProvider.getDataDir(), "indexes", "rulebooks");
        this.embeddingService = embeddingService;
        try {
            Files.createDirectories(indexPath);
        } catch (IOException e) {
            throw new IllegalStateException("Failed to create index directory", e);
        }
    }

    @Override
    public void indexRulebookChunks(RulebookId rulebookId, String filename, List<String> chunks) throws IOException {
        ensureIndexCompatibility();
        try (IndexWriter writer = createWriter()) {
            addChunks(writer, rulebookId, filename, chunks);
            writer.commit();
        }
    }

    @Override
    public List<RulebookContext> search(String query, int topK, Set<RulebookId> enabledRulebookIds) throws IOException {
        if (enabledRulebookIds.isEmpty()) {
            return List.of();
        }
        ensureIndexCompatibility();
        Set<String> enabledIds = enabledRulebookIds.stream()
                .map(RulebookId::value)
                .collect(java.util.stream.Collectors.toSet());
        float[] queryVector = embeddingService.embed(query);
        return searchEnabledContexts(enabledIds, queryVector, topK);
    }

    private IndexWriter createWriter() throws IOException {
        Directory directory = FSDirectory.open(indexPath);
        IndexWriterConfig config = new IndexWriterConfig(new StandardAnalyzer());
        config.setOpenMode(IndexWriterConfig.OpenMode.CREATE_OR_APPEND);
        return new IndexWriter(directory, config);
    }

    private void addChunks(IndexWriter writer, RulebookId rulebookId, String filename, List<String> chunks) throws IOException {
        for (String chunk : chunks) {
            writer.addDocument(toDocument(rulebookId, filename, chunk));
        }
    }

    private Document toDocument(RulebookId rulebookId, String filename, String chunk) {
        Document doc = new Document();
        doc.add(new StringField("rulebookId", rulebookId.value(), Field.Store.YES));
        doc.add(new StringField("filename", filename, Field.Store.YES));
        doc.add(new StoredField("text", chunk));
        doc.add(new KnnVectorField("embedding", embeddingService.embed(chunk)));
        return doc;
    }

    private List<RulebookContext> searchEnabledContexts(Set<String> enabledIds, float[] queryVector, int topK) throws IOException {
        Directory directory = FSDirectory.open(indexPath);
        if (!DirectoryReader.indexExists(directory)) {
            return List.of();
        }
        try (DirectoryReader reader = DirectoryReader.open(directory)) {
            return collectSearchResults(reader, enabledIds, queryVector, topK);
        }
    }

    private List<RulebookContext> collectSearchResults(DirectoryReader reader,
                                                       Set<String> enabledIds,
                                                       float[] queryVector,
                                                       int topK) throws IOException {
        IndexSearcher searcher = new IndexSearcher(reader);
        TopDocs topDocs = searcher.search(new KnnVectorQuery("embedding", queryVector, topK * 3), topK * 3);
        return toRulebookContexts(searcher, topDocs.scoreDocs, enabledIds, topK);
    }

    private List<RulebookContext> toRulebookContexts(IndexSearcher searcher,
                                                     ScoreDoc[] scoreDocs,
                                                     Set<String> enabledIds,
                                                     int topK) throws IOException {
        List<RulebookContext> results = new ArrayList<>();
        for (ScoreDoc scoreDoc : scoreDocs) {
            if (appendIfEnabled(results, searcher.doc(scoreDoc.doc), enabledIds) && results.size() >= topK) {
                return results;
            }
        }
        return results;
    }

    private boolean appendIfEnabled(List<RulebookContext> results, Document doc, Set<String> enabledIds) {
        String rulebookId = doc.get("rulebookId");
        if (!enabledIds.contains(rulebookId)) {
            return false;
        }
        results.add(new RulebookContext(RulebookId.of(rulebookId), doc.get("filename"), doc.get("text")));
        return true;
    }

    private void ensureIndexCompatibility() throws IOException {
        synchronized (indexLock) {
            Properties expected = expectedMetadata();
            Properties current = readMetadata();
            if (isCompatible(current, expected)) {
                return;
            }
            resetIndexDirectory();
            writeMetadata(expected);
        }
    }

    private Properties expectedMetadata() {
        Properties props = new Properties();
        props.setProperty(META_PROVIDER, embeddingService.provider());
        props.setProperty(META_MODEL_VERSION, embeddingService.modelVersion());
        props.setProperty(META_DIMENSION, String.valueOf(embeddingService.dimension()));
        return props;
    }

    private Properties readMetadata() throws IOException {
        Path metaPath = indexPath.resolve(META_FILE);
        Properties props = new Properties();
        if (!Files.exists(metaPath)) {
            return props;
        }
        try (InputStream in = Files.newInputStream(metaPath)) {
            props.load(in);
        }
        return props;
    }

    private boolean isCompatible(Properties current, Properties expected) {
        if (current.isEmpty()) {
            return false;
        }
        return expected.getProperty(META_PROVIDER).equals(current.getProperty(META_PROVIDER))
                && expected.getProperty(META_MODEL_VERSION).equals(current.getProperty(META_MODEL_VERSION))
                && expected.getProperty(META_DIMENSION).equals(current.getProperty(META_DIMENSION));
    }

    private void writeMetadata(Properties props) throws IOException {
        Path metaPath = indexPath.resolve(META_FILE);
        try (OutputStream out = Files.newOutputStream(metaPath)) {
            props.store(out, "Rulebook index metadata");
        }
    }

    private void resetIndexDirectory() throws IOException {
        if (!Files.exists(indexPath)) {
            Files.createDirectories(indexPath);
            return;
        }
        try (var walk = Files.walk(indexPath)) {
            walk.sorted(Comparator.reverseOrder())
                    .filter(path -> !path.equals(indexPath))
                    .forEach(this::deletePath);
        }
        Files.createDirectories(indexPath);
    }

    private void deletePath(Path path) {
        try {
            Files.deleteIfExists(path);
        } catch (IOException e) {
            throw new IllegalStateException("Failed to reset index path: " + path, e);
        }
    }
}
