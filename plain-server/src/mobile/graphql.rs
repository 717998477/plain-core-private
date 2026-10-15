use crate::graphql::resolvers::packages::public::PackagesMutation;
use crate::graphql::resolvers::packages::public::PackagesQuery;
use crate::graphql::resolvers::sms::SmsMutation;
use crate::graphql::resolvers::sms::SmsQuery;
use crate::mobile::contacts::resolvers::ContactsMutation;
use crate::mobile::contacts::resolvers::ContactsQuery;
use crate::mobile::phone_calls::resolvers::CallsMutation;
use crate::mobile::phone_calls::resolvers::CallsQuery;
use async_graphql::{EmptySubscription, MergedObject, SchemaBuilder};
// Not named `Query`: async-graphql derives a merged object's GraphQL name
// from its Rust type name, and this one is merged into `public::Query`.
#[derive(MergedObject, Default)]
pub struct MobileQuery(PackagesQuery, ContactsQuery, SmsQuery, CallsQuery);
// Not named `Mutation`, for the same reason as `MobileQuery`.
#[derive(MergedObject, Default)]
pub struct MobileMutation(
    PackagesMutation,
    ContactsMutation,
    SmsMutation,
    CallsMutation,
);
pub(crate) fn configure(
    builder: SchemaBuilder<
        crate::graphql::schemas::public::Query,
        crate::graphql::schemas::public::Mutation,
        EmptySubscription,
    >,
    runtime: std::sync::Arc<super::Runtime>,
) -> SchemaBuilder<
    crate::graphql::schemas::public::Query,
    crate::graphql::schemas::public::Mutation,
    EmptySubscription,
> {
    builder.data(runtime.mms.clone()).data(std::sync::Arc::new(
        super::phone_calls::geo::Runtime::default(),
    ))
}
