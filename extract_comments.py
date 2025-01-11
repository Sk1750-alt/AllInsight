import requests
import json

# Replace with your valid short-lived access token obtained through OAuth.
ACCESS_TOKEN = "IGAAQSp03E1b9BZAE5GcWkwNWRUelRoeWF2NmlnUGk1a1d2QnJXQUFHUWlQbi1OY2o4WFVfU1RGQ285TFludHBnZA3UwMDloQjBFU2pRWUgzVzNtbGpvR1VBQW9xRDhWQTZA4aVRIUFZAEZAFpDZAHBiQ2VLU1oxVmZAmVGpnTVFjbGRMSQZDZD"

def get_media_ids(access_token):
    """
    Retrieve media IDs associated with the user's account.
    """
    url = f"https://graph.instagram.com/me/media/?fields=id,caption&access_token={access_token}"
    try:
        response = requests.get(url)
        response.raise_for_status()
        data = response.json()
        return data.get("data", [])
    except requests.exceptions.RequestException as e:
        print(f"Error fetching media IDs: {e}")
        return []

def get_post_data(post_id, access_token):
    """
    Retrieve post data and comments using the given post (media) ID.
    """
    url = f"https://graph.instagram.com/{post_id}?fields=caption,comments{{text,username}}&access_token={access_token}"
    try:
        response = requests.get(url)
        response.raise_for_status()
        data = response.json()
        
        # Extract comments if available
        comments = data.get("comments", {}).get("data", [])
        return data, comments
    except requests.exceptions.RequestException as e:
        print(f"Error fetching post data: {e}")
        try:
            error_data = response.json()
            print(f"Error details: {json.dumps(error_data, indent=2)}")
        except Exception:
            print("Could not parse error details.")
        return None, None
    except json.JSONDecodeError as e:
        print(f"Error decoding JSON response: {e}")
        return None, None
    except Exception as e:
        print(f"An unexpected error occurred: {e}")
        return None, None

# Step 1: Retrieve Media IDs
print("Fetching media IDs...")
media_list = get_media_ids(ACCESS_TOKEN)
if not media_list:
    print("No media found or could not retrieve media IDs. Ensure your token has the required permissions.")
    exit()

print("Media IDs retrieved:")
for media in media_list:
    print(f"ID: {media['id']}, Caption: {media.get('caption', 'No Caption')}")

# Step 2: Use the first media ID for demonstration
POST_ID = "DCzeWUcOI43"  # Replace with the actual Media ID of your post.
print(f"\nUsing Media ID: {POST_ID}")

# Step 3: Fetch Post Data and Comments
print("\nFetching post data and comments...")
post_data, comments = get_post_data(POST_ID, ACCESS_TOKEN)

if post_data:
    print("\nPost Data:")
    print(json.dumps(post_data, indent=2))
else:
    print("Could not retrieve post data.")

if comments:
    print("\nComments:")
    for comment in comments:
        print(json.dumps(comment, indent=2))
        print("-" * 10)
else:
    print("\nNo comments found or could not retrieve comments.")
