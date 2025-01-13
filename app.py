import streamlit as st
import torch
from model_utils import load_model, predict_sentiment
import os

# Check if running in a Colab environment
if 'COLAB_TPU_ADDR' in os.environ:
    MODEL_PATH = "/content/sentiment_model/sentiment_model"  # Path in Colab
else:
    MODEL_PATH = "sentiment_model" # Path for local running. Make sure the folder exists.
device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

@st.cache_resource
def load_model_and_tokenizer(model_path):
    model, tokenizer = load_model(model_path)
    if model is not None and tokenizer is not None:
        model.to(device)
    return model, tokenizer

model, tokenizer = load_model_and_tokenizer(MODEL_PATH)

if model is None or tokenizer is None:
    st.error("Model loading failed. Please check the model path.")
    st.stop()

st.title("Sentiment Analysis App")

user_text = st.text_area("Enter text for sentiment analysis:", height = 200)

if st.button("Analyze Sentiment"):
    if not user_text:
        st.warning("Please enter some text to analyze.")
    else:
        with st.spinner("Analyzing sentiment..."):
            sentiment_label, probabilities = predict_sentiment(model, tokenizer, user_text, device)

        st.subheader("Results:")
        st.write(f"Predicted sentiment: **{sentiment_label}**")
        st.write(f"Positive probability: {probabilities[1]:.2f}")
        st.write(f"Negative probability: {probabilities[0]:.2f}")