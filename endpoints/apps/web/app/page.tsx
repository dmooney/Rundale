import { CreatorConsole, FirebaseCreatorConsole } from "./creator-console";
import { firebaseIsConfigured } from "./firebase";

export default function HomePage() {
  return firebaseIsConfigured() ? <FirebaseCreatorConsole /> : <CreatorConsole />;
}
