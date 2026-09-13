ALTER TABLE "deployment_aliases" DROP CONSTRAINT "deployment_aliases_endpoint_draft_id_endpoint_drafts_id_fk";
--> statement-breakpoint
ALTER TABLE "deployment_aliases" ALTER COLUMN "endpoint_version_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "invocations" ALTER COLUMN "endpoint_version_id" DROP NOT NULL;--> statement-breakpoint
ALTER TABLE "invocations" ADD COLUMN "endpoint_draft_id" uuid;--> statement-breakpoint
ALTER TABLE "invocations" ADD CONSTRAINT "invocations_endpoint_draft_id_endpoint_drafts_id_fk" FOREIGN KEY ("endpoint_draft_id") REFERENCES "public"."endpoint_drafts"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "deployment_aliases" DROP COLUMN "endpoint_draft_id";