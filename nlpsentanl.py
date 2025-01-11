# -*- coding: utf-8 -*-
"""NLPsentanl.ipynb

Automatically generated by Colab.

Original file is located at
    https://colab.research.google.com/drive/1LZJ8vliRXjtRS-6QHjfDCEshQb_xiNI6
"""

import pandas as pd
from sklearn.model_selection import train_test_split
from sklearn.metrics import accuracy_score, classification_report
from transformers import BertTokenizer, BertForSequenceClassification
from transformers import Trainer, TrainingArguments
import torch
from torch.utils.data import Dataset, DataLoader
import os
import matplotlib.pyplot as plt
os.environ["WANDB_DISABLED"] = "true"

# Load datasets
train_file = "your_sentiment_data.csv"
test_file = "Politics.csv"

train_data = pd.read_csv(train_file)
test_data = pd.read_csv(test_file)

# Define a custom dataset class
class SentimentDataset(Dataset):
    def __init__(self, texts, labels, tokenizer, max_length=128):
        self.texts = texts
        self.labels = labels
        self.tokenizer = tokenizer
        self.max_length = max_length

    def __len__(self):
        return len(self.texts)

    def __getitem__(self, idx):
        text = self.texts[idx]
        label = self.labels[idx]
        encoding = self.tokenizer.encode_plus(
            text,
            add_special_tokens=True,
            max_length=self.max_length,
            truncation=True,
            padding="max_length",
            return_tensors="pt"
        )
        return {
            'input_ids': encoding['input_ids'].squeeze(0),
            'attention_mask': encoding['attention_mask'].squeeze(0),
            'labels': torch.tensor(label, dtype=torch.long)
        }

# Load tokenizer and define datasets
tokenizer = BertTokenizer.from_pretrained('bert-base-uncased')

train_texts = train_data['text'].tolist()
train_labels = train_data['label'].tolist()

test_texts = test_data['text'].tolist()
test_labels = [1 if label == "positive" else 0 for label in test_data['label'].tolist()]

train_dataset = SentimentDataset(train_texts, train_labels, tokenizer)
test_dataset = SentimentDataset(test_texts, test_labels, tokenizer)

# Define the model
model = BertForSequenceClassification.from_pretrained('bert-base-uncased', num_labels=2)

# Define training arguments
training_args = TrainingArguments(
    output_dir="./results",
    num_train_epochs=3,
    per_device_train_batch_size=16,
    per_device_eval_batch_size=16,
    warmup_steps=500,
    weight_decay=0.01,
    logging_dir="./logs",
    logging_steps=10,
    evaluation_strategy="epoch"
)

# Define Trainer
trainer = Trainer(
    model=model,
    args=training_args,
    train_dataset=train_dataset,
    eval_dataset=test_dataset, # Add this line to pass the evaluation dataset
    tokenizer=tokenizer
)

# Train the model
trainer.train()

"""Calculate the accuracy of the model"""

# Evaluate on the test dataset
def evaluate_model(model, test_dataset, device): # Add device argument
    test_dataloader = DataLoader(test_dataset, batch_size=16)
    model.eval()
    predictions = []
    true_labels = []
    with torch.no_grad():
        for batch in test_dataloader:
            input_ids = batch['input_ids'].to(device) # Move to device
            attention_mask = batch['attention_mask'].to(device) # Move to device
            labels = batch['labels'].to(device) # Move to device
            outputs = model(input_ids=input_ids, attention_mask=attention_mask)
            logits = outputs.logits
            preds = torch.argmax(logits, dim=1).detach().cpu().numpy()
            predictions.extend(preds)
            true_labels.extend(labels.cpu().numpy())
    return predictions, true_labels

# Get the device
device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
model.to(device) # Move the model to the device

# Call evaluate_model with the device
predictions, true_labels = evaluate_model(model, test_dataset, device)

# Calculate accuracy
accuracy = accuracy_score(true_labels, predictions)
report = classification_report(true_labels, predictions)

print(f"Accuracy: {accuracy}")
print(f"Classification Report:\n{report}")
accuracy_percentage = accuracy * 100
print(f"Accuracy: {accuracy_percentage}%")

"""Downloading the trained model in zip format"""

import shutil
import os

output_dir = "./results"  # Your model's output directory
archive_name = "trained_model" # Name of the archive (without extension)
archive_format = "zip" # Format of the archive (zip, tar, gztar, bztar)

# Create the archive
shutil.make_archive(archive_name, archive_format, output_dir)

print(f"Model saved as {archive_name}.{archive_format}")

#Clean up the directory after archiving (optional)
shutil.rmtree(output_dir)
print(f"Directory {output_dir} removed.")

"""Run the trained model with suitable example"""

import shutil
import os
from transformers import BertTokenizer, BertForSequenceClassification
import torch

def analyze_sentiment(text, model, tokenizer, device):
    inputs = tokenizer(text, padding=True, truncation=True, return_tensors="pt").to(device)
    with torch.no_grad():
        outputs = model(**inputs)
    probabilities = torch.nn.functional.softmax(outputs.logits, dim=-1)
    predicted_class = torch.argmax(probabilities).item()
    sentiment = "Positive" if predicted_class == 1 else "Negative"
    return sentiment, probabilities[0].cpu().numpy()

# Extract the archive (if not already extracted)
archive_name = "trained_model.zip"
extract_path = "extracted_model"

if not os.path.exists(extract_path):
    try:
        shutil.unpack_archive(archive_name, extract_path)
        print(f"Model extracted to {extract_path}") # Informative message
    except FileNotFoundError:
        print(f"Error: Archive {archive_name} not found. Please ensure it is in the correct directory.")
        exit() # Exit gracefully if the archive is not found
    except Exception as e:
        print(f"Error extracting archive: {e}")
        exit()
else:
    print(f"Model already extracted in {extract_path}")


# Load the model and tokenizer
device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
try:
    tokenizer = BertTokenizer.from_pretrained(extract_path)
    model = BertForSequenceClassification.from_pretrained(extract_path).to(device)
    model.eval()
    print("Model and Tokenizer loaded successfully.")
except Exception as e:
    print(f"Error loading model or tokenizer: {e}")
    exit()

# Example usage (you can modify this for interactive input)
examples = [
    "This product is absolutely fantastic! I highly recommend it.",
    "I had a terrible experience. The service was awful and the product was defective.",
    "The movie was okay, nothing special.",
    "This is a very complex issue with no easy solutions.",
    "The new software update is a game-changer for our workflow." #Added one more example
]

print("\nStarting Sentiment Analysis Demo:\n") #Clearer start message
for i, example in enumerate(examples):
    sentiment, probabilities = analyze_sentiment(example, model, tokenizer, device)
    print(f"Example {i+1}:") #Numbered examples for clarity
    print(f"Text: \"{example}\"")
    print(f"Sentiment: {sentiment}")
    print(f"Positive Probability: {probabilities[1]:.4f}") #More readable probability output
    print(f"Negative Probability: {probabilities[0]:.4f}")
    print("-" * 30)

#Optional cleanup (Only after the Demo is successfully finished)
cleanup = input("Do you want to clean up the extracted model files? (yes/no): ")
if cleanup.lower() == "yes":
    try:
        shutil.rmtree(extract_path)
        print(f"Directory {extract_path} removed.")
    except Exception as e:
        print(f"Error removing directory: {e}")
else:
    print("Cleanup skipped.")

print("\nDemo Complete.") #Clear end message

