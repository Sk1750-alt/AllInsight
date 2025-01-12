import torch
from model_utils import load_model, predict_sentiment  # Import the functions

MODEL_PATH = "/content/sentiment_model/sentiment_model"
device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

# Load the model and tokenizer using the function
model, tokenizer = load_model(MODEL_PATH)

if model is None or tokenizer is None:
    print("Model loading failed. Exiting.")
    exit()

model.to(device)

# Example usage (you'll use this in your Streamlit app)
statement = "This is a great movie!"
sentiment, probabilities = predict_sentiment(model, tokenizer, statement, device)
print(f"Sentiment: {sentiment}")
print(f"Probabilities: {probabilities}")