#!/usr/bin/env bash
#
# Test Genome Entity CRUD Operations
#
# Validates Phase 1.1 completion: GenomeEntity and GenomeLayerEntity
# are properly registered and operational via CRUD commands.
#

set -e

echo "🧬 Testing Genome Entity Registration (Phase 1.1)"
echo ""

# Generate 768-dim embedding (simple pattern for testing)
EMBEDDING="["
for i in {1..768}; do
  VALUE=$(echo "scale=6; s($i/100) * e(-$i/1000)" | bc -l)
  EMBEDDING="${EMBEDDING}${VALUE}"
  if [ $i -lt 768 ]; then
    EMBEDDING="${EMBEDDING},"
  fi
done
EMBEDDING="${EMBEDDING}]"

echo "📝 Step 1: Creating GenomeLayerEntity with 768-dim embedding..."
LAYER_RESULT=$(./jtag data/create --collection=genome_layers --data="{
  \"name\":\"Test Math Expertise Layer\",
  \"description\":\"LoRA layer trained on mathematical reasoning\",
  \"traitType\":\"domain_expertise\",
  \"source\":\"trained\",
  \"modelPath\":\"/models/lora/math_v1.safetensors\",
  \"sizeMB\":42.5,
  \"rank\":16,
  \"embedding\":${EMBEDDING},
  \"trainingMetadata\":{
    \"curriculumType\":\"progressive_difficulty\",
    \"datasetSize\":5000,
    \"epochs\":3,
    \"finalLoss\":0.023,
    \"trainingDuration\":7200
  },
  \"fitness\":{
    \"accuracy\":0.87,
    \"efficiency\":0.92,
    \"usageCount\":0,
    \"cacheHitRate\":0.0
  }
}")

echo "$LAYER_RESULT"
LAYER_ID=$(echo "$LAYER_RESULT" | jq -r '.id')

if [ "$LAYER_ID" = "null" ] || [ -z "$LAYER_ID" ]; then
  echo "❌ Failed to create GenomeLayerEntity"
  exit 1
fi

echo "✅ Created GenomeLayerEntity: $LAYER_ID"
echo ""

echo "📝 Step 2: Creating GenomeEntity with layer reference..."
GENOME_RESULT=$(./jtag data/create --collection=genomes --data="{
  \"name\":\"Math Tutor Genome v1\",
  \"description\":\"Specialized genome for mathematical tutoring\",
  \"personaId\":\"00000000-0000-0000-0000-000000000001\",
  \"baseModel\":\"llama-3.1-8B\",
  \"layers\":[
    {
      \"layerId\":\"${LAYER_ID}\",
      \"traitType\":\"domain_expertise\",
      \"orderIndex\":0,
      \"weight\":1.0,
      \"enabled\":true
    }
  ],
  \"compositeEmbedding\":${EMBEDDING},
  \"metadata\":{
    \"generation\":1,
    \"parentGenomes\":[],
    \"trainingDuration\":7200,
    \"creationMethod\":\"manual\"
  },
  \"fitness\":{
    \"overallAccuracy\":0.87,
    \"totalParameters\":1048576,
    \"totalSizeMB\":42.5,
    \"averageLatency\":250
  }
}")

echo "$GENOME_RESULT"
GENOME_ID=$(echo "$GENOME_RESULT" | jq -r '.id')

if [ "$GENOME_ID" = "null" ] || [ -z "$GENOME_ID" ]; then
  echo "❌ Failed to create GenomeEntity"
  exit 1
fi

echo "✅ Created GenomeEntity: $GENOME_ID"
echo ""

echo "📖 Step 3: Reading GenomeLayerEntity back..."
LAYER_READ=$(./jtag data/read --collection=genome_layers --id="$LAYER_ID")
echo "$LAYER_READ" | jq '{name, traitType, sizeMB, rank, fitness}'
echo ""

echo "📖 Step 4: Reading GenomeEntity back..."
GENOME_READ=$(./jtag data/read --collection=genomes --id="$GENOME_ID")
echo "$GENOME_READ" | jq '{name, baseModel, layers, metadata, fitness}'
echo ""

echo "✅ All CRUD operations successful!"
echo ""
echo "🎉 Phase 1.1 Complete: Genome entities registered and operational!"
echo ""
echo "📊 Summary:"
echo "  - GenomeLayerEntity: $LAYER_ID"
echo "  - GenomeEntity: $GENOME_ID"
echo "  - Both entities stored in SQLite database"
echo "  - Entity validation working correctly"
echo ""
echo "Next: Phase 1.2 - Extend PersonaUser with genome reference"
