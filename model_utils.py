import torch
from transformers import BertTokenizer, BertForSequenceClassification

def load_model(model_path):
    """Loads the pre-trained BERT model and tokenizer."""
    try:
        model = BertForSequenceClassification.from_pretrained(model_path)
        tokenizer = BertTokenizer.from_pretrained(model_path)
        return model, tokenizer
    except Exception as e:  # Handle potential errors during loading
        print(f"Error loading model: {e}")
        return None, None

def predict_sentiment(model, tokenizer, statement, device):
    """Predicts the sentiment of a given statement."""
    encoding = tokenizer(statement, return_tensors="pt", truncation=True, padding="max_length", max_length=128).to(device)
    with torch.no_grad():
        outputs = model(input_ids=encoding['input_ids'], attention_mask=encoding['attention_mask'])
        logits = outputs.logits
        probs = torch.softmax(logits, dim=1)
        sentiment = torch.argmax(probs).item()
        sentiment_label = "Positive" if sentiment == 1 else "Negative"
        return sentiment_label, probs[0].cpu().numpy()