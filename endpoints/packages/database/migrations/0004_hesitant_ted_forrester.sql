ALTER TABLE "deployment_aliases" DROP CONSTRAINT "deployment_aliases_endpoint_version_id_endpoint_versions_id_fk";
--> statement-breakpoint
ALTER TABLE "invocations" DROP CONSTRAINT "invocations_endpoint_version_id_endpoint_versions_id_fk";
--> statement-breakpoint
ALTER TABLE "invocations" DROP CONSTRAINT "invocations_endpoint_draft_id_endpoint_drafts_id_fk";
--> statement-breakpoint
ALTER TABLE "endpoint_drafts" ADD CONSTRAINT "endpoint_drafts_endpoint_id_id_unique" UNIQUE("endpoint_id","id");--> statement-breakpoint
ALTER TABLE "endpoint_versions" ADD CONSTRAINT "endpoint_versions_endpoint_id_id_unique" UNIQUE("endpoint_id","id");--> statement-breakpoint
ALTER TABLE "deployment_aliases" ADD CONSTRAINT "deployment_aliases_endpoint_version_endpoint_fk" FOREIGN KEY ("endpoint_id","endpoint_version_id") REFERENCES "public"."endpoint_versions"("endpoint_id","id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "invocations" ADD CONSTRAINT "invocations_endpoint_version_endpoint_fk" FOREIGN KEY ("endpoint_id","endpoint_version_id") REFERENCES "public"."endpoint_versions"("endpoint_id","id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "invocations" ADD CONSTRAINT "invocations_endpoint_draft_endpoint_fk" FOREIGN KEY ("endpoint_id","endpoint_draft_id") REFERENCES "public"."endpoint_drafts"("endpoint_id","id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "invocations" ADD CONSTRAINT "invocations_exactly_one_source_check" CHECK (("invocations"."endpoint_version_id" IS NOT NULL) <> ("invocations"."endpoint_draft_id" IS NOT NULL));
